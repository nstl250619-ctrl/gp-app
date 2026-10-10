//! Shop 集成（v1.1 冻结口径，2026-10-07 联调接入）。
//! 职责边界：Shop = 账号/钱包(USD)/订单/发货真源；new-api = 额度/兑换码核销；
//! 两套钱包相互独立；兑换码核销只走 new-api（POST /api/user/topup）。
//!
//! 口径：信封 {success,message,data} 失败同为 HTTP 200；金额字符串两位小数 USD；
//! 充值 7.7 的 amount 为 ¥ 实付，折算率以 7.5 的 rate 下发（勿硬编码）；
//! id 均为纯数字字符串；除 7.1~7.4 外 Bearer session_token。

use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::config;
use crate::credential;
use crate::error::{AppError, AppResult};

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopStatus {
    pub ready: bool,
    pub base_url: String,
    /// SSO 端点是否已上线（哑探测，不消耗真 token）。
    pub sso_ready: bool,
    pub message: String,
}

#[derive(Debug, Clone, Deserialize)]
struct SessionData {
    #[serde(default)]
    session_token: String,
    #[serde(default)]
    #[allow(dead_code)] // v1.1 契约字段，设备会话长期复用，暂不消费
    expires_in: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopUser {
    #[serde(default, alias = "user_id")]
    pub user_id: i64,
    #[serde(default)]
    pub email: String,
    #[serde(default)]
    pub username: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletSummary {
    pub user: ShopUser,
    /// USD 余额（字符串两位小数原样）。
    pub balance: String,
    pub currency: String,
    /// 人民币→USD 折算率（1 USD = rate ¥）。
    pub rate: String,
    /// 折算展示：¥ = USD × rate。
    pub balance_yuan: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedeemResult {
    pub quota_granted: i64,
    /// USD 计量（与 new-api 一致）：quota / 500000
    pub granted_usd: f64,
}

fn http() -> reqwest::Client {
    reqwest::Client::builder()
        // Shop 未就绪/慢时快速失败，避免拖慢登录与总览探测（原 30s 会让界面干等）
        .timeout(Duration::from_secs(8))
        .no_proxy() // 直连：系统代理可能错误路由 *.greenpool.cn（SSO 铸号被劫走），实测直连正常
        .build()
        .expect("shop http client")
}

fn url(path: &str) -> String {
    format!("{}{}", config::SHOP_BASE_URL, path)
}

/// /api/* 统一信封（与 new-api 同构）：认 success，不认 HTTP 状态码。
async fn parse_envelope<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> AppResult<T> {
    let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| AppError::Other("商城响应无法解析".into()))?;
    let data = v.get("data").cloned().unwrap_or(serde_json::Value::Null);
    if v.get("success").and_then(|s| s.as_bool()).unwrap_or(false) {
        serde_json::from_value(data).map_err(|_| AppError::Other("商城响应数据格式不符".into()))
    } else {
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("商城操作未成功");
        Err(AppError::Other(msg.to_string()))
    }
}

fn device_id() -> AppResult<String> {
    if let Some(d) = credential::get("device_id")? {
        return Ok(d);
    }
    let d = uuid::Uuid::new_v4().to_string();
    credential::set("device_id", &d)?;
    Ok(d)
}

/// 设备会话（7.1）：无则建，有则复用。返回 session_token。
async fn device_session() -> AppResult<String> {
    if let Some(s) = credential::get(config::SHOP_SESSION_TOKEN)? {
        return Ok(s);
    }
    let resp = http()
        .post(url("/api/session"))
        .json(&json!({ "device_id": device_id()?, "app_version": env!("CARGO_PKG_VERSION") }))
        .send()
        .await
        .map_err(AppError::Network)?;
    let data: SessionData = parse_envelope(resp).await?;
    if data.session_token.is_empty() {
        return Err(AppError::Other("商城会话建立失败".into()));
    }
    credential::set(config::SHOP_SESSION_TOKEN, &data.session_token)?;
    Ok(data.session_token)
}

/// SSO 端点是否已部署（哑探测：伪 token 探 404 与否，绝不消耗真 token）。
async fn sso_exists() -> bool {
    match http()
        .get(url("/api/sso"))
        .query(&[("session_token", "probe")])
        .send()
        .await
    {
        // 404=未部署；400/401=已部署（伪 token 被拒，正是预期）
        Ok(resp) => resp.status().as_u16() != 404,
        Err(_) => false,
    }
}

/// 真实状态探测：能建立会话即视为商城已上线。
pub async fn status() -> ShopStatus {
    match device_session().await {
        Ok(_) => ShopStatus {
            ready: true,
            base_url: config::SHOP_BASE_URL.into(),
            sso_ready: sso_exists().await,
            message: String::new(),
        },
        Err(e) => ShopStatus {
            ready: false,
            base_url: config::SHOP_BASE_URL.into(),
            sso_ready: false,
            message: e.to_string(),
        },
    }
}

/// 生成 SSO 内嵌地址（方案 A）：每次静默重登 Shop 铸**新** session_token，
/// 兼容 Shop 侧"一次性 token"的最严格安全模式。
pub async fn sso_url() -> AppResult<String> {
    let identifier = credential::get(config::CREDENTIAL_ACCOUNT_EMAIL)?
        .or_else(|| credential::get(config::CREDENTIAL_ACCOUNT_USERNAME).ok().flatten())
        .ok_or_else(|| AppError::Other("请先在总览登录账号".into()))?;
    let password = stored_shop_password()
        .ok_or_else(|| AppError::Other("请先在总览登录账号".into()))?;
    client_login(&identifier, &password).await?;
    let token = shop_auth_token().ok_or_else(|| AppError::Other("商城登录态获取失败".into()))?;
    Ok(format!("{}/api/sso?session_token={}", config::SHOP_BASE_URL, token))
}

/// 登录态 token（7.3/7.4 返回的 session_token，非设备会话）。
fn shop_auth_token() -> Option<String> {
    credential::get(config::SHOP_AUTH_TOKEN).ok().flatten()
}

/// Shop 登录密码优先级：绑定时的自定义 Shop 密码 → 退回 new-api 密码。
pub fn stored_shop_password() -> Option<String> {
    credential::get(config::SHOP_PASSWORD)
        .ok()
        .flatten()
        .or_else(|| credential::get(config::CREDENTIAL_ACCOUNT_PASSWORD).ok().flatten())
}

/// Shop 设备会话失效判定（Shop 侧重启/清理会话后，存量 device session 全部失效，
/// 所有请求报「会话已过期，请重启工具」）。
fn is_session_error(msg: &str) -> bool {
    msg.contains("会话") || msg.contains("重启工具")
}

/// Shop 登录（7.4）。设备会话失效时自动重建并重试一次。
pub async fn client_login(identifier: &str, password: &str) -> AppResult<ShopUser> {
    match client_login_inner(identifier, password).await {
        Ok(u) => Ok(u),
        Err(e) => {
            if !is_session_error(&e.to_string()) {
                return Err(e);
            }
            // 重建设备会话后重试一次
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            client_login_inner(identifier, password).await
        }
    }
}

async fn client_login_inner(identifier: &str, password: &str) -> AppResult<ShopUser> {
    #[derive(Deserialize)]
    struct LoginData {
        #[serde(flatten)]
        user: ShopUser,
        #[serde(default)]
        session_token: String,
    }
    let resp = http()
        .post(url("/api/user/login"))
        .json(&json!({ "email": identifier, "password": password, "session_token": device_session().await? }))
        .send()
        .await
        .map_err(AppError::Network)?;
    let data: LoginData = parse_envelope(resp).await?;
    if !data.session_token.is_empty() {
        credential::set(config::SHOP_AUTH_TOKEN, &data.session_token)?;
    }
    if data.user.user_id != 0 {
        credential::set(config::SHOP_USER_ID, &data.user.user_id.to_string())?;
    }
    Ok(data.user)
}

/// 我的资料 + 钱包（7.5，Bearer 登录态）。会话失效自动用同凭据重登一次再试。
pub async fn wallet() -> AppResult<WalletSummary> {
    match wallet_inner().await {
        Ok(w) => Ok(w),
        Err(e) => {
            let m = e.to_string();
            let expired = m.contains("会话") || m.contains("过期") || m.contains("未登录")
                || m.to_lowercase().contains("unauthorized") || m.to_lowercase().contains("not logged in");
            if !expired {
                return Err(e);
            }
            // 静默重登 Shop（同一套凭据；Shop 账号是邮箱体系，优先用邮箱），换新登录态后重试一次
            let identifier = credential::get(config::CREDENTIAL_ACCOUNT_EMAIL)?
                .or_else(|| credential::get(config::CREDENTIAL_ACCOUNT_USERNAME).ok().flatten())
                .ok_or_else(|| AppError::SessionExpired)?;
            let password = stored_shop_password()
                .ok_or_else(|| AppError::SessionExpired)?;
            client_login(&identifier, &password).await?;
            wallet_inner().await
        }
    }
}

async fn wallet_inner() -> AppResult<WalletSummary> {
    let token = shop_auth_token().ok_or_else(|| AppError::Other("商城账号未登录".into()))?;
    #[derive(Deserialize)]
    struct MeData {
        #[serde(flatten)]
        user: ShopUser,
        #[serde(default)]
        wallet: WalletField,
    }
    #[derive(Deserialize, Default)]
    struct WalletField {
        #[serde(default)]
        balance: String,
        #[serde(default)]
        currency: String,
        #[serde(default)]
        rate: String,
    }
    let resp = http()
        .get(url("/api/user/me"))
        .bearer_auth(&token)
        .send()
        .await
        .map_err(AppError::Network)?;
    let data: MeData = parse_envelope(resp).await?;
    let balance_yuan = match (data.wallet.balance.parse::<f64>(), data.wallet.rate.parse::<f64>()) {
        (Ok(b), Ok(r)) if r > 0.0 => format!("{:.2}", b * r),
        _ => String::new(),
    };
    Ok(WalletSummary {
        user: data.user,
        balance: data.wallet.balance,
        currency: data.wallet.currency,
        rate: data.wallet.rate,
        balance_yuan,
    })
}

/// Shop 账号是否已登录（供三端关联）。
pub fn linked() -> bool {
    shop_auth_token().is_some()
}

/// 注册发码（Shop /api/email/code scene=register）。设备会话失效自动重建重试。
/// 用户收一个码，Shop 注册时后端同步在 new-api 建号，一次注册两套账号。
pub async fn send_register_code(email: &str) -> AppResult<()> {
    match send_register_code_inner(email).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if !is_session_error(&e.to_string()) {
                return Err(e);
            }
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            send_register_code_inner(email).await
        }
    }
}

async fn send_register_code_inner(email: &str) -> AppResult<()> {
    let resp = http()
        .post(url("/api/email/code"))
        .json(&json!({ "email": email, "scene": "register", "session_token": device_session().await? }))
        .send()
        .await
        .map_err(AppError::Network)?;
    parse_envelope::<serde_json::Value>(resp).await.map(|_| ())
}

/// Shop 注册（POST /api/user/register）。Shop 后端同步在 new-api 建号（admin create）。
/// 邀请码自动带入（config::SHOP_INVITE_CODE）；返回的登录态存钥匙串。设备会话失效自动重建重试。
pub async fn register(email: &str, code: &str, password: &str) -> AppResult<()> {
    match register_inner(email, code, password).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if !is_session_error(&e.to_string()) {
                return Err(e);
            }
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            register_inner(email, code, password).await
        }
    }
}

async fn register_inner(email: &str, code: &str, password: &str) -> AppResult<()> {
    #[derive(Deserialize)]
    struct RegData {
        #[serde(default)]
        session_token: String,
        #[serde(default)]
        user_id: i64,
    }
    let resp = http()
        .post(url("/api/user/register"))
        .json(&json!({
            "email": email,
            "code": code,
            "password": password,
            "invite_code": config::SHOP_INVITE_CODE,
            "session_token": device_session().await?
        }))
        .send()
        .await
        .map_err(AppError::Network)?;
    let data: RegData = parse_envelope(resp).await?;
    if !data.session_token.is_empty() {
        credential::set(config::SHOP_AUTH_TOKEN, &data.session_token)?;
    }
    if data.user_id != 0 {
        credential::set(config::SHOP_USER_ID, &data.user_id.to_string())?;
    }
    Ok(())
}

/// Shop SMTP 发码（绑定场景，scene=register）。设备会话失效自动重建重试。
pub async fn send_bind_code() -> AppResult<()> {
    match send_bind_code_inner().await {
        Ok(()) => Ok(()),
        Err(e) => {
            if !is_session_error(&e.to_string()) {
                return Err(e);
            }
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            send_bind_code_inner().await
        }
    }
}

async fn send_bind_code_inner() -> AppResult<()> {
    let email = credential::get(config::CREDENTIAL_ACCOUNT_EMAIL)?
        .ok_or_else(|| AppError::Other("账号没有邮箱，无法绑定商城".into()))?;
    let resp = http()
        .post(url("/api/email/code"))
        .json(&json!({ "email": email, "scene": "register", "session_token": device_session().await? }))
        .send()
        .await
        .map_err(AppError::Network)?;
    parse_envelope::<serde_json::Value>(resp).await.map(|_| ())
}

/// 绑定 Shop 账号（老账号路径）：用当前账号的邮箱在 Shop 注册（建钱包）。
/// Shop 报「已注册」则直接登录补齐关联。绑定不影响 new-api 登录态。
/// shop_password：Shop 密码与 new-api 不同时由用户填入；缺省用 new-api 密码。
pub async fn bind(code: String, shop_password: Option<String>) -> AppResult<WalletSummary> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(AppError::Other("请输入邮箱验证码".into()));
    }
    let email = credential::get(config::CREDENTIAL_ACCOUNT_EMAIL)?
        .ok_or_else(|| AppError::Other("账号没有邮箱，无法绑定商城".into()))?;
    let password = match shop_password.map(|p| p.trim().to_string()).filter(|p| !p.is_empty()) {
        Some(p) => {
            // 自定义 Shop 密码持久化（后续静默重登优先用它，不再退回 new-api 密码）
            credential::set(config::SHOP_PASSWORD, &p)?;
            p
        }
        None => stored_shop_password()
            .ok_or_else(|| AppError::Other("请重新登录后再绑定".into()))?,
    };

    let mut body = json!({ "email": email, "code": code, "password": password, "session_token": device_session().await? });
    let mut v = register_attempt(body.clone()).await?;
    if !v.get("success").and_then(|s| s.as_bool()).unwrap_or(false) {
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("").to_string();
        if is_session_error(&msg) {
            // 设备会话失效：重建后重试一次
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            body = json!({ "email": email, "code": code, "password": password, "session_token": device_session().await? });
            v = register_attempt(body).await?;
        }
    }
    let success = v.get("success").and_then(|s| s.as_bool()).unwrap_or(false);
    if success {
        if let Some(st) = v.pointer("/data/session_token").and_then(|s| s.as_str()) {
            credential::set(config::SHOP_AUTH_TOKEN, st)?;
        }
        if let Some(uid) = v.pointer("/data/user_id") {
            credential::set(config::SHOP_USER_ID, &uid.to_string())?;
        }
    } else {
        let msg = v.get("message").and_then(|m| m.as_str()).unwrap_or("商城绑定未成功").to_string();
        let m = msg.to_lowercase();
        let taken = msg.contains("已注册") || msg.contains("已存在") || m.contains("exist") || m.contains("taken");
        if !taken {
            return Err(AppError::Other(msg));
        }
        // Shop 已有该邮箱：直接登录补齐关联
        client_login(&email, &password).await?;
    }
    wallet().await
}

/// Shop 注册请求（返回原始 JSON，成功/失败语义由调用方判定）。
async fn register_attempt(body: serde_json::Value) -> AppResult<serde_json::Value> {
    let resp = http()
        .post(url("/api/user/register"))
        .json(&body)
        .send()
        .await
        .map_err(AppError::Network)?;
    let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
    serde_json::from_str(&text).map_err(|_| AppError::Other("商城响应无法解析".into()))
}

/// 兑换（真核销）：登录态直调 new-api，Shop 不参与。
pub async fn redeem(code: String) -> AppResult<RedeemResult> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(AppError::Other("请输入兑换码".into()));
    }
    let token = crate::services::account::ensure_session().await?;
    let client = crate::newapi::NewApiClient::with_token(config::DEFAULT_BASE_URL, &token);
    let quota = client.redeem_code(&code).await?;
    Ok(RedeemResult { quota_granted: quota, granted_usd: config::quota_to_usd(quota) })
}

/// 登出：清 Shop 登录态（设备会话保留，避免重复建会话）。
pub fn logout() -> AppResult<()> {
    let _ = credential::delete(config::SHOP_AUTH_TOKEN);
    let _ = credential::delete(config::SHOP_USER_ID);
    Ok(())
}
