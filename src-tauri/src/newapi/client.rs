//! new-api HTTP 客户端（精简版）：仅保留中转站状态探测（status）与密钥连通自测（verify_key）。
//! 账号/密钥/用量/兑换/找回密码已全部切网关（Shop 网关代理 new-api），此处不再直连。

use serde_json::{json, Value};

use crate::config::DEFAULT_BASE_URL;
use crate::error::{AppError, AppResult};

use super::types::InstanceStatus;

#[derive(Clone)]
pub struct NewApiClient {
    base_url: String,
}

impl NewApiClient {
    pub fn new(base_url: &str) -> Self {
        Self { base_url: base_url.trim_end_matches('/').to_string() }
    }

    fn http(&self) -> reqwest::Client {
        // 全局单例：跨调用复用连接池。
        static SHARED: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
        SHARED
            .get_or_init(|| crate::services::http_client(10, 30)) // 直连探测：建连 10s、总超时 30s
            .clone()
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
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

    /// 密钥连通自测：用用户 key 调 /v1/chat/completions（ping），无 error 即连通。
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
