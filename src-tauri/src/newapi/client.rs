//! new-api HTTP 客户端（§8）。只调公开 API，绝不直插 DB。
//! 错误判定：/api/* 认 success 字段、/v1/* 认 error 字段，不认 HTTP 状态码。

use std::time::Duration;

use serde::de::DeserializeOwned;
use serde_json::{json, Value};

use crate::config::DEFAULT_BASE_URL;
use crate::error::{AppError, AppResult};

use super::types::{Envelope, InstanceStatus, LoginData, SelfData, TokenListItem, TokenPage};

#[derive(Clone)]
pub struct NewApiClient {
    base_url: String,
    access_token: Option<String>,
}

impl NewApiClient {
    pub fn new(base_url: &str) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), access_token: None }
    }

    pub fn with_token(base_url: &str, access_token: &str) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string(), access_token: Some(access_token.to_string()) }
    }

    fn http(&self) -> reqwest::Client {
        // 全局单例 + cookie_store：跨调用保留 refresh cookie。
        // JWT 过期时走 /auth/refresh 轮换会话（服务端 rotate 不新建会话），根治会话数堆积（50 上限）。
        static SHARED: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| {
                reqwest::Client::builder()
                    .connect_timeout(Duration::from_secs(10)) // 建连快速失败，别让用户干等 30s
                    .timeout(Duration::from_secs(30))
                    .cookie_store(true)
                    .no_proxy() // 直连：系统代理（Clash 等）可能把 *.greenpool.cn 错误路由，实测 SSO 被劫到 new-api
                    .build()
                    .expect("failed to build http client")
            })
            .clone()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }

    fn get(&self, path: &str) -> reqwest::RequestBuilder {
        let mut b = self.http().get(self.url(path));
        if let Some(t) = &self.access_token {
            b = b.bearer_auth(t);
        }
        b
    }

    fn post(&self, path: &str) -> reqwest::RequestBuilder {
        let mut b = self.http().post(self.url(path));
        if let Some(t) = &self.access_token {
            b = b.bearer_auth(t);
        }
        b
    }

    async fn parse_envelope<T: DeserializeOwned>(resp: reqwest::Response) -> AppResult<T> {
        let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
        let env: Envelope<T> = serde_json::from_str(&text)
            .map_err(|_| AppError::Other("中转站响应无法解析".into()))?;
        if env.success {
            env.data.ok_or_else(|| AppError::Other("中转站响应缺少数据".into()))
        } else {
            let fallback = if env.message.trim().is_empty() { "中转站操作未成功" } else { &env.message };
            Err(AppError::Other(match &env.code {
                Some(c) if !c.is_empty() => Self::humanize_code(c, fallback),
                _ => fallback.to_string(),
            }))
        }
    }

    pub async fn get_json<T: DeserializeOwned>(&self, path: &str) -> AppResult<T> {
        let resp = self.get(path).send().await.map_err(AppError::Network)?;
        Self::parse_envelope(resp).await
    }

    pub async fn post_json<T: DeserializeOwned>(&self, path: &str, body: Value) -> AppResult<T> {
        let resp = self.post(path).json(&body).send().await.map_err(AppError::Network)?;
        Self::parse_envelope(resp).await
    }

    /// 已知错误码 → 人话（新版 new-api 安全会话体系；code 优先于原始 message）。
    fn humanize_code(code: &str, fallback: &str) -> String {
        match code {
            "AUTH_SESSION_LIMIT" => {
                "登录会话数已达上限：请在已登录的网页端「登录会话」撤销其他会话，或联系管理员清理后重试".into()
            }
            "AUTH_SESSION_ISSUANCE_LIMIT" => "登录过于频繁：服务器限流中，请稍后再试".into(),
            "AUTH_SESSION_REVOKED" | "AUTH_TOKEN_EXPIRED" => "登录状态已失效，请重新登录".into(),
            "AUTH_SESSION_MISMATCH" | "AUTH_REFRESH_RACE" => "登录状态冲突，请重新登录".into(),
            "USERNAME_OR_PASSWORD_INCORRECT" | "BAD_CREDENTIALS" => "账号或密码错误".into(),
            _ => fallback.to_string(),
        }
    }

    /// 仅校验 success 的响应（部分接口成功不返回 data，如 POST /api/token/）。
    async fn expect_ok(resp: reqwest::Response) -> AppResult<()> {
        let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
        let env: Envelope<Value> = serde_json::from_str(&text)
            .map_err(|_| AppError::Other("中转站响应无法解析".into()))?;
        if env.success {
            Ok(())
        } else {
            let fallback = if env.message.trim().is_empty() { "中转站操作未成功" } else { &env.message };
            Err(AppError::Other(match &env.code {
                Some(c) if !c.is_empty() => Self::humanize_code(c, fallback),
                _ => fallback.to_string(),
            }))
        }
    }

    async fn post_expect_ok(&self, path: &str, body: Value) -> AppResult<()> {
        let resp = self.post(path).json(&body).send().await.map_err(AppError::Network)?;
        Self::expect_ok(resp).await
    }

    /// GET /api/verification?email=... 触发站点 SMTP 发码（公开；30 秒 2 次/IP 限速，
    /// 域白名单与别名限制由站点校验，错误人话透传）。
    pub async fn send_verification_code(&self, email: &str) -> AppResult<()> {
        let resp = self
            .http()
            .get(self.url("/api/verification"))
            .query(&[("email", email)])
            .send()
            .await
            .map_err(AppError::Network)?;
        Self::expect_ok(resp).await
    }

    /// POST /api/user/register（公开）。站点当前 EmailVerificationEnabled=true，
    /// 必须带 email + verification_code；username ≤20 字、password ≥8 由本端先校验。
    pub async fn register(&self, username: &str, password: &str, email: &str, code: &str) -> AppResult<()> {
        self.post_expect_ok(
            "/api/user/register",
            json!({
                "username": username,
                "password": password,
                "email": email,
                "verification_code": code
            }),
        )
        .await
    }

    /// GET /api/reset_password?email=... 发送重置邮件（防枚举：无论邮箱是否存在都返回 success）。
    pub async fn send_password_reset(&self, email: &str) -> AppResult<()> {
        let resp = self
            .http()
            .get(self.url("/api/reset_password"))
            .query(&[("email", email)])
            .send()
            .await
            .map_err(AppError::Network)?;
        Self::expect_ok(resp).await
    }

    /// POST /api/user/reset {email, token} → data = 站点生成的新密码。
    pub async fn reset_password(&self, email: &str, token: &str) -> AppResult<String> {
        self.post_json("/api/user/reset", json!({ "email": email, "token": token })).await
    }

    /// POST /api/user/auth/logout 撤销当前服务端会话（新版 new-api 会话体系）。
    /// 防止会话堆积撞上 USER_SESSION_ACTIVE_LIMIT（默认 50）。
    pub async fn auth_logout(&self) -> AppResult<()> {
        let resp = self.post("/api/user/auth/logout").send().await.map_err(AppError::Network)?;
        let _ = resp.text().await;
        Ok(())
    }

    /// GET /api/status（公开）。兼容 {data} 包裹或直接返回两种情况。
    pub async fn status(&self) -> AppResult<InstanceStatus> {
        let resp = self.http().get(self.url("/api/status")).send().await.map_err(AppError::Network)?;
        let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).map_err(|_| AppError::Other("中转站状态响应无法解析".into()))?;
        let data = v.get("data").unwrap_or(&v);
        let s = |k: &str| data.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let b = |k: &str| data.get(k).and_then(|x| x.as_bool()).unwrap_or(false);
        Ok(InstanceStatus {
            version: s("version"),
            server_address: s("server_address"),
            email_verification: b("email_verification"),
            self_use_mode: b("self_use_mode"),
            register_enabled: b("register_enabled"),
            turnstile_check: b("turnstile_check"),
        })
    }

    pub async fn login(&self, username: &str, password: &str) -> AppResult<LoginData> {
        self.post_json("/api/user/login", json!({ "username": username, "password": password })).await
    }

    pub async fn self_info(&self) -> AppResult<SelfData> {
        self.get_json("/api/user/self").await
    }

    /// 按名字搜索当前用户的令牌（列表条目为脱敏，无明文）。
    pub async fn search_tokens(&self, keyword: &str) -> AppResult<Vec<TokenListItem>> {
        let resp = self
            .get("/api/token/search")
            .query(&[("keyword", keyword), ("p", "1"), ("size", "50")])
            .send()
            .await
            .map_err(AppError::Network)?;
        let page: TokenPage = Self::parse_envelope(resp).await?;
        Ok(page.items)
    }

    /// 建令牌（无限额度、跟随用户分组）。成功响应只有 success（无 data），id 需 search 二次定位。
    pub async fn create_token(&self, name: &str) -> AppResult<()> {
        self.post_expect_ok(
            "/api/token/",
            json!({
                "name": name,
                "remain_quota": 0,
                "unlimited_quota": true,
                "expired_time": -1,
                "model_limits_enabled": false,
                "model_limits": "",
                "group": "",
                "status": 1
            }),
        )
        .await
    }

    /// 当前用户全部令牌（GET /api/token/，分页取前 100；条目脱敏无明文）。
    pub async fn list_tokens(&self) -> AppResult<Vec<TokenListItem>> {
        let resp = self
            .get("/api/token/")
            .query(&[("p", "1"), ("size", "100")])
            .send()
            .await
            .map_err(AppError::Network)?;
        let page: super::types::PageData<TokenListItem> = Self::parse_envelope(resp).await?;
        Ok(page.items.unwrap_or_default())
    }

    /// 当前用户计费日志（GET /api/log/self，type=2 只取消费记录）。
    /// 计费日志（type=2 消费）。
    pub async fn logs(&self, page: i64, page_size: i64, start_ts: i64, end_ts: i64) -> AppResult<(Vec<super::types::LogItem>, i64)> {
        self.logs_by_type(2, page, page_size, start_ts, end_ts).await
    }

    /// 按类型取日志（type=1 充值 / type=2 消费）。start/end 为 0 表示不过滤时间。
    pub async fn logs_by_type(&self, log_type: i64, page: i64, page_size: i64, start_ts: i64, end_ts: i64) -> AppResult<(Vec<super::types::LogItem>, i64)> {
        let resp = self
            .get("/api/log/self")
            .query(&[
                ("p", page.to_string()),
                ("page_size", page_size.to_string()),
                ("type", log_type.to_string()),
                ("start_timestamp", start_ts.to_string()),
                ("end_timestamp", end_ts.to_string()),
            ])
            .send()
            .await
            .map_err(AppError::Network)?;
        let page_data: super::types::LogPage = Self::parse_envelope(resp).await?;
        Ok((page_data.items.unwrap_or_default(), page_data.total))
    }

    /// 兑换码核销（POST /api/user/topup {key}）→ data = 到账 quota。
    /// 失败统一返回"兑换失败"（站点防枚举设计，不区分细分原因）。
    pub async fn redeem_code(&self, code: &str) -> AppResult<i64> {
        self.post_json("/api/user/topup", json!({ "key": code })).await
    }

    /// 刷新会话（POST /api/user/auth/refresh）：服务端轮换同一会话的 refresh secret（rotate），
    /// 返回新 access token——**不新建会话**，根治会话数堆积（USER_SESSION_ACTIVE_LIMIT 50）。
    /// 依赖共享 cookie store 里的 refresh cookie（login 时服务端 Set-Cookie 写入，轮换后自动更新）。
    pub async fn refresh_auth(&self) -> AppResult<String> {
        let data: super::types::LoginData =
            self.post_json("/api/user/auth/refresh", json!({})).await?;
        Ok(data.access_token)
    }

    /// 查兑换码有效期（GET /api/user/redemption/{id}，服务器补丁接口；校验 used_user_id 为本人）。
    /// 接口未上线（404）或任何失败时返回 None，调用方优雅降级——绝不因它阻塞兑换记录展示。
    pub async fn redemption_expired_time(&self, id: i64) -> Option<i64> {
        let resp = self.get(&format!("/api/user/redemption/{}", id)).send().await.ok()?;
        if !resp.status().is_success() {
            return None;
        }
        let v: Value = resp.json().await.ok()?;
        v.get("data")?.get("expired_time")?.as_i64()
    }

    /// 当前账号可用的模型列表（GET /api/user/models）。
    /// 站点内部已按账号分组过滤（GetUserUsableGroups）并对同名模型去重。
    pub async fn user_models(&self) -> AppResult<Vec<String>> {
        let resp = self
            .get("/api/user/models")
            .send()
            .await
            .map_err(AppError::Network)?;
        Self::parse_envelope(resp).await
    }

    /// 当前用户最近充值记录（GET /api/user/topup/self）。
    pub async fn topup_records(&self) -> AppResult<Vec<super::types::TopUpRecord>> {
        let resp = self
            .get("/api/user/topup/self")
            .query(&[("p", "1"), ("size", "10")])
            .send()
            .await
            .map_err(AppError::Network)?;
        let page: super::types::PageData<super::types::TopUpRecord> = Self::parse_envelope(resp).await?;
        Ok(page.items.unwrap_or_default())
    }

    /// 取令牌明文（POST /api/token/:id/key → data.key）。
    pub async fn get_token_key(&self, id: i64) -> AppResult<String> {
        #[derive(serde::Deserialize)]
        struct KeyData {
            #[serde(default)]
            key: String,
        }
        let data: KeyData = self.post_json(&format!("/api/token/{}/key", id), json!({})).await?;
        if data.key.is_empty() {
            Err(AppError::Other("令牌密钥为空".into()))
        } else {
            Ok(data.key)
        }
    }

    /// 连接自测：真发一次最小补全请求。/v1 认 error 字段。
    pub async fn verify_key(&self, key: &str, model: &str) -> AppResult<bool> {
        let resp = self
            .http()
            .post(self.url("/v1/chat/completions"))
            .bearer_auth(key)
            .json(&json!({ "model": model, "messages": [{ "role": "user", "content": "ping" }], "max_tokens": 1 }))
            .send()
            .await
            .map_err(AppError::Network)?;
        let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
        let v: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({ "error": "bad response" }));
        Ok(v.get("error").is_none())
    }

    pub fn default_client() -> Self {
        Self::new(DEFAULT_BASE_URL)
    }
}

/// base64url 解码（JWT payload 用；无外部依赖，容错 '=' 填充与缺位）。
fn b64url_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'-' => Some(62),
        b'_' => Some(63),
        _ => None,
    }
}

fn b64url_decode(input: &str) -> Option<Vec<u8>> {
    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut v = [0u8; 4];
        for (i, &c) in chunk.iter().enumerate() {
            v[i] = b64url_val(c)?;
        }
        match chunk.len() {
            2 => out.push((v[0] << 2) | (v[1] >> 4)),
            3 => {
                out.push((v[0] << 2) | (v[1] >> 4));
                out.push((v[1] << 4) | (v[2] >> 2));
            }
            4 => {
                out.push((v[0] << 2) | (v[1] >> 4));
                out.push((v[1] << 4) | (v[2] >> 2));
                out.push((v[2] << 6) | v[3]);
            }
            _ => return None,
        }
    }
    Some(out)
}

/// 解析 JWT 的 exp（秒）。token 形如 header.payload.signature；解析失败返回 None
/// （调用方应回退到服务端判定，绝不因本地解析失败而误判有效/无效）。
pub fn jwt_exp(token: &str) -> Option<i64> {
    let payload = token.split('.').nth(1)?;
    let bytes = b64url_decode(payload)?;
    let v: Value = serde_json::from_slice(&bytes).ok()?;
    v.get("exp").and_then(|e| e.as_i64())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jwt_exp_parses_payload() {
        // payload = base64url('{"exp":1}') = "eyJleHAiOjF9"
        assert_eq!(jwt_exp("a.eyJleHAiOjF9.c"), Some(1));
        assert_eq!(jwt_exp("a.e30.c"), None); // payload '{}'
        assert_eq!(jwt_exp("garbage"), None);
        assert_eq!(jwt_exp(""), None);
    }
}
