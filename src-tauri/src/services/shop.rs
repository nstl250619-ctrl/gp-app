//! Shop 集成（v1.1 冻结口径，2026-10-07 联调接入）。
//! 职责边界：Shop = 账号/钱包(USD)/订单/发货真源；new-api = 额度/兑换码核销；
//! 两套钱包相互独立；兑换码核销只走 new-api（POST /api/user/topup）。
//!
//! 口径：信封 {success,message,data} 失败同为 HTTP 200；金额字符串两位小数 USD；
//! 充值 7.7 的 amount 为 ¥ 实付，折算率以 7.5 的 rate 下发（勿硬编码）；
//! id 均为纯数字字符串；除 7.1~7.4 外 Bearer session_token。

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
    /// 是否触发订阅自动购买（按兑换码套餐名映射）。
    pub subscription_active: bool,
    /// 激活的订阅套餐 id（2=尝鲜 3=周卡 4=月卡 5=日卡；None=未激活订阅）。
    pub plan_id: Option<i64>,
}

fn http() -> reqwest::Client {
    // 全局单例：复用连接池与 TLS 会话。每调用新建 Client 会在并发（如明细并行翻页）
    // 时产生 8+ 个独立连接池，突发建连失败直接表现为「网络连不上中转站」。
    static SHARED: std::sync::OnceLock<reqwest::Client> = std::sync::OnceLock::new();
    SHARED
        .get_or_init(|| super::http_client(8, 15)) // 连接 8s 快速失败；总超时 15s 给慢响应余量
        .clone()
}

fn url(path: &str) -> String {
    format!("{}{}", config::SHOP_BASE_URL, path)
}

/// /api/* 统一信封（与 new-api 同构）：认 success，不认 HTTP 状态码。
async fn parse_envelope<T: serde::de::DeserializeOwned>(resp: reqwest::Response) -> AppResult<T> {
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
    let v: serde_json::Value = serde_json::from_str(&text)
        .map_err(|_| AppError::Other("商城响应无法解析".into()))?;
    // 401 + 业务 code → 绑定引导（契约见《联合整改需求单 v2》4.1）：
    // 用户只知道「请先登录」却不知道要绑定中转站账号，必须转成可识别的错误码。
    if status == 401 {
        match v.get("code").and_then(|c| c.as_str()).unwrap_or("") {
            "need_bind_newapi" => return Err(AppError::NeedBindNewapi),
            "need_rebind_newapi" => return Err(AppError::NeedRebindNewapi),
            "newapi_user_banned" => return Err(AppError::NewapiBanned),
            // 注册时邮箱已存在（商城 401 + code）：转 EMAIL_TAKEN，前端自动切登录 tab 并预填账号
            "already_registered" => return Err(AppError::EmailTaken),
            _ => {}
        }
    }
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

/// 生成 SSO 内嵌地址（方案 A）：复用现有登录态 token（避免每次点击商品卡都重登录 Shop），
/// 仅在无 token 时才静默登录铸新 token。SSO 端点已改为可复用（限流 10/min 防滥用）。
pub async fn sso_url() -> AppResult<String> {
    if let Some(token) = shop_auth_token() {
        return Ok(format!("{}/api/sso?session_token={}", config::SHOP_BASE_URL, token));
    }
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

/// 找回密码发码（Shop /api/email/code scene=reset）。设备会话失效自动重建重试。
pub async fn send_reset_code(email: &str) -> AppResult<()> {
    match send_reset_code_inner(email).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if !is_session_error(&e.to_string()) {
                return Err(e);
            }
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            send_reset_code_inner(email).await
        }
    }
}

async fn send_reset_code_inner(email: &str) -> AppResult<()> {
    let resp = http()
        .post(url("/api/email/code"))
        .json(&json!({ "email": email, "scene": "reset", "session_token": device_session().await? }))
        .send()
        .await
        .map_err(AppError::Network)?;
    parse_envelope::<serde_json::Value>(resp).await.map(|_| ())
}

/// 找回密码重置（POST /api/user/reset）：改 Shop 密码 + 网关同步改 new-api 密码（直写 SQLite）。
/// 成功后不自动登录（前端拿到新密码后自行登录）。设备会话失效自动重建重试。
pub async fn reset_password(email: &str, code: &str, new_password: &str) -> AppResult<()> {
    match reset_password_inner(email, code, new_password).await {
        Ok(()) => Ok(()),
        Err(e) => {
            if !is_session_error(&e.to_string()) {
                return Err(e);
            }
            let _ = credential::delete(config::SHOP_SESSION_TOKEN);
            reset_password_inner(email, code, new_password).await
        }
    }
}

async fn reset_password_inner(email: &str, code: &str, new_password: &str) -> AppResult<()> {
    let resp = http()
        .post(url("/api/user/reset"))
        .json(&json!({
            "email": email,
            "code": code,
            "new_password": new_password,
            "session_token": device_session().await?
        }))
        .send()
        .await
        .map_err(AppError::Network)?;
    parse_envelope::<serde_json::Value>(resp).await.map(|_| ())
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

/// 兑换（真核销）：走网关（含 48h 试用自动购买），Shop 网关代理 new-api。
pub async fn redeem(code: String) -> AppResult<RedeemResult> {
    let code = code.trim().to_string();
    if code.is_empty() {
        return Err(AppError::Other("请输入兑换码".into()));
    }
    gateway_redeem(&code).await
}

/// 登出：清 Shop 登录态（设备会话保留，避免重复建会话）。
pub fn logout() -> AppResult<()> {
    let _ = credential::delete(config::SHOP_AUTH_TOKEN);
    let _ = credential::delete(config::SHOP_USER_ID);
    Ok(())
}

// ==================== 网关 new-api 代理（P2 收敛：额度/模型/密钥/用量/兑换统一走网关） ====================

fn gateway_token() -> AppResult<String> {
    shop_auth_token().ok_or_else(|| AppError::Other("请先登录".into()))
}

/// 静默重登：用存储的邮箱+密码重建登录态（网关重启/会话过期后自愈，用户无感）。
/// 全局互斥：并发请求同时发现会话失效时只重登一次，避免登录风暴与钥匙串并发写。
async fn gateway_relogin() -> AppResult<()> {
    static LOCK: std::sync::OnceLock<tokio::sync::Mutex<()>> = std::sync::OnceLock::new();
    let lock = LOCK.get_or_init(|| tokio::sync::Mutex::new(()));
    let _guard = lock.lock().await;
    let identifier = credential::get(config::CREDENTIAL_ACCOUNT_EMAIL)?
        .or_else(|| credential::get(config::CREDENTIAL_ACCOUNT_USERNAME).ok().flatten())
        .ok_or(AppError::SessionExpired)?;
    let password = stored_shop_password().ok_or(AppError::SessionExpired)?;
    client_login(&identifier, &password).await?;
    Ok(())
}

async fn gateway_get<T: serde::de::DeserializeOwned>(path: &str) -> AppResult<T> {
    match gateway_get_once(path).await {
        Err(e) if is_session_error(&e.to_string()) => {
            // 会话失效（网关重启/过期）：静默重登后重试一次
            gateway_relogin().await?;
            gateway_get_once(path).await
        }
        r => r,
    }
}

async fn gateway_get_once<T: serde::de::DeserializeOwned>(path: &str) -> AppResult<T> {
    let resp = http()
        .get(url(path))
        .bearer_auth(&gateway_token()?)
        .send()
        .await
        .map_err(AppError::Network)?;
    parse_envelope(resp).await
}

/// 绑定中转站账号（《联合整改需求单 v2》4.2）：POST /api/tool/newapi/bind。
/// 仅在中转站**已有**该账号（SSO 返回 need_bind_newapi）时使用：
/// 网关校验通过后加密保存密码并立即写入 tool_sessions.new_api_token，之后自动登录。
/// 响应体兼容「裸 JSON」（{ok,bound}）与「success/data 信封」两种形态。
pub async fn bind_newapi(username: String, password: String) -> AppResult<()> {
    let resp = http()
        .post(url("/api/tool/newapi/bind"))
        .bearer_auth(&gateway_token()?)
        .json(&json!({ "newapi_username": username, "newapi_password": password }))
        .send()
        .await
        .map_err(AppError::Network)?;
    let status = resp.status().as_u16();
    let text = resp.text().await.map_err(|e| AppError::Other(e.to_string()))?;
    let v: serde_json::Value = serde_json::from_str(&text).unwrap_or(serde_json::Value::Null);
    if (200..300).contains(&status) {
        // 2xx 还不够：商城可能用 200 包裹 { ok:false } / { success:false } 的业务失败，
        // 直接放行会导致「弹框关闭 → 仍 401 → 反复弹框」。
        let explicit_fail = v.get("ok").and_then(|b| b.as_bool()) == Some(false)
            || v.get("success").and_then(|b| b.as_bool()) == Some(false);
        if !explicit_fail {
            return Ok(());
        }
    }
    // 401 credentials_invalid / 400 等：回传服务端具体原因（密码错 / 账号不存在 / 被禁用）
    let msg = v
        .get("message")
        .or_else(|| v.pointer("/error/message"))
        .and_then(|m| m.as_str())
        .unwrap_or("绑定失败，请检查中转站账号与密码");
    Err(AppError::Other(msg.to_string()))
}

async fn gateway_post<T: serde::de::DeserializeOwned>(path: &str, body: serde_json::Value) -> AppResult<T> {
    match gateway_post_once(path, body.clone()).await {
        Err(e) if is_session_error(&e.to_string()) => {
            gateway_relogin().await?;
            gateway_post_once(path, body).await
        }
        r => r,
    }
}

async fn gateway_post_once<T: serde::de::DeserializeOwned>(path: &str, body: serde_json::Value) -> AppResult<T> {
    let resp = http()
        .post(url(path))
        .bearer_auth(&gateway_token()?)
        .json(&body)
        .send()
        .await
        .map_err(AppError::Network)?;
    parse_envelope(resp).await
}

/// new-api self（余额/用量），经网关 /api/user/quota 代理。
pub async fn gateway_self() -> AppResult<crate::newapi::SelfData> {
    gateway_get("/api/user/quota").await
}

/// 订阅明细：激活订阅的配额/已用/到期（quota 原始单位）。
/// 经网关 /api/user/subscription（new-api subscription/self 代理）。
/// 数据形如 { subscriptions: [{ subscription: { amount_total, amount_used, end_time } }] }（嵌套）。
/// 多个激活订阅时取剩余最多的一个（当前业务只会有一个生效订阅）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SubscriptionInfo {
    pub total: i64,
    pub used: i64,
    pub end_time: i64,
}

impl SubscriptionInfo {
    pub fn remaining(&self) -> i64 {
        (self.total - self.used).max(0)
    }
}

pub async fn gateway_subscription() -> AppResult<Option<SubscriptionInfo>> {
    #[derive(Deserialize)]
    struct SubItem {
        #[serde(default)]
        subscription: Option<SubInner>,
    }
    #[derive(Deserialize)]
    struct SubInner {
        #[serde(default, alias = "amount_total")]
        amount_total: i64,
        #[serde(default, alias = "amount_used")]
        amount_used: i64,
        #[serde(default, alias = "end_time")]
        end_time: i64,
    }
    #[derive(Deserialize)]
    struct SubData {
        #[serde(default, alias = "subscriptions")]
        subscriptions: Vec<SubItem>,
    }
    let d: SubData = gateway_get("/api/user/subscription").await?;
    Ok(d
        .subscriptions
        .iter()
        .filter_map(|s| s.subscription.as_ref())
        .map(|s| SubscriptionInfo { total: s.amount_total, used: s.amount_used, end_time: s.end_time })
        .max_by_key(|s| s.remaining()))
}

/// 账号可用模型（分组过滤+去重），经网关 /api/user/models。
pub async fn gateway_models() -> AppResult<Vec<String>> {
    gateway_get("/api/user/models").await
}

/// 令牌列表（脱敏），经网关 /api/user/keys。
pub async fn gateway_list_keys() -> AppResult<Vec<crate::newapi::TokenListItem>> {
    gateway_get("/api/user/keys").await
}

/// 建令牌，经网关 /api/user/keys。
pub async fn gateway_create_key(name: &str) -> AppResult<()> {
    gateway_post::<serde_json::Value>("/api/user/keys", json!({ "name": name }))
        .await
        .map(|_| ())
}

/// 取令牌明文，经网关 /api/user/keys/:id/plain。
pub async fn gateway_key_plain(id: i64) -> AppResult<String> {
    #[derive(Deserialize)]
    struct KeyData {
        #[serde(default)]
        key: String,
    }
    let d: KeyData = gateway_get(&format!("/api/user/keys/{}/plain", id)).await?;
    if d.key.is_empty() {
        Err(AppError::Other("令牌密钥为空".into()))
    } else {
        Ok(d.key)
    }
}

/// 用量日志（经网关 /api/user/usage）。log_type：2=成功计费 5=失败；page 从 1 起（200 条/页）。
pub async fn gateway_usage(start_ts: i64, end_ts: i64, page: i64, log_type: i64) -> AppResult<(Vec<crate::newapi::LogItem>, i64)> {
    #[derive(Deserialize)]
    struct UsageData {
        #[serde(default)]
        items: Vec<crate::newapi::LogItem>,
        #[serde(default)]
        total: i64,
    }
    let d: UsageData = gateway_get(&format!(
        "/api/user/usage?start_ts={}&end_ts={}&page={}&type={}",
        start_ts, end_ts, page, log_type
    ))
    .await?;
    Ok((d.items, d.total))
}

/// 用量聚合（网关 /api/user/usage/summary）：1 个请求拿到 明细前100 + 总数 + 失败数 + 按天积分。
/// 翻页与聚合在网关侧完成（同机内网），替代桌面端 N 次翻页往返。
pub async fn gateway_usage_summary(
    start_ts: i64,
    end_ts: i64,
) -> AppResult<crate::services::usage::GatewayUsageSummary> {
    gateway_get(&format!("/api/user/usage/summary?start_ts={}&end_ts={}", start_ts, end_ts))
        .await
}

/// 最近充值记录，经网关 /api/user/topups。
pub async fn gateway_topups() -> AppResult<Vec<crate::newapi::TopUpRecord>> {
    gateway_get("/api/user/topups").await
}

/// 兑换记录（type=1 日志），经网关 /api/user/redemptions。
pub async fn gateway_redemptions() -> AppResult<Vec<crate::newapi::LogItem>> {
    gateway_get("/api/user/redemptions").await
}

/// 兑换码核销（含 48h 试用），经网关 /api/user/redeem。
pub async fn gateway_redeem(code: &str) -> AppResult<RedeemResult> {
    #[derive(Deserialize)]
    struct SubscriptionField {
        #[serde(default)]
        active: bool,
        #[serde(default)]
        plan_id: Option<i64>,
    }
    #[derive(Deserialize)]
    struct RedeemData {
        #[serde(default)]
        quota: i64,
        #[serde(default)]
        subscription: Option<SubscriptionField>,
    }
    let d: RedeemData = gateway_post("/api/user/redeem", json!({ "code": code })).await?;
    Ok(RedeemResult {
        quota_granted: d.quota,
        granted_usd: config::quota_to_usd(d.quota),
        subscription_active: d.subscription.as_ref().map(|s| s.active).unwrap_or(false),
        plan_id: d.subscription.and_then(|s| s.plan_id),
    })
}

/// 撤销网关会话（清 new-api 会话 + 解绑），经网关 /api/user/logout。失败不阻断（会话 24h 自动过期）。
pub async fn gateway_logout() {
    let _ = gateway_post::<serde_json::Value>("/api/user/logout", json!({})).await;
}

// ==================== 网关商城（P3：商品列表 / 余额直购） ====================

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductItem {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub price: f64,
    #[serde(default, alias = "compare_price")]
    pub compare_price: Option<f64>,
    #[serde(default)]
    pub description: String,
    #[serde(default, alias = "fulfill_type")]
    pub fulfill_type: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkuItem {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub price: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ProductDetail {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub price: f64,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub skus: Vec<SkuItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BuyResult {
    #[serde(default, alias = "order_no")]
    pub order_no: String,
    #[serde(default)]
    pub amount: f64,
    #[serde(default)]
    pub status: String,
    #[serde(default, alias = "delivery_text")]
    pub delivery_text: String,
    /// 一键购买自动化：网关是否已就地核销兑换码
    #[serde(default)]
    pub redeemed: bool,
    /// 自动激活的订阅套餐 id（2=尝鲜 3=周卡 4=月卡 5=日卡；激活失败为 None）
    #[serde(default, alias = "plan_id")]
    pub plan_id: Option<i64>,
    /// 自动核销到账额度（USD）
    #[serde(default, alias = "granted_usd")]
    pub granted_usd: f64,
}

/// 工具商城展示的商品白名单（Shop 后台 goods id）。
/// 仅展示绿池自有卡密商品：日卡 → 周卡 → 月卡（按周期递增）。
/// Shop 后台新增商品需同步此列表；无关商品（号码类/补差价等）不进入工具商城。
const FEATURED_GOODS: [i64; 3] = [9, 7, 8];

/// 商品列表（网关 /api/products，按白名单过滤并按周期排序）。
pub async fn gateway_products() -> AppResult<Vec<ProductItem>> {
    #[derive(Deserialize)]
    struct ListData {
        #[serde(default)]
        list: Vec<ProductItem>,
    }
    let d: ListData = gateway_get("/api/products").await?;
    let mut list = d.list;
    list.retain(|g| FEATURED_GOODS.contains(&g.id));
    list.sort_by_key(|g| FEATURED_GOODS.iter().position(|&id| id == g.id).unwrap_or(usize::MAX));
    Ok(list)
}

/// 商品详情（含 sku，网关 /api/products/:id）。
pub async fn gateway_product_detail(id: i64) -> AppResult<ProductDetail> {
    gateway_get(&format!("/api/products/{}", id)).await
}

/// 余额直购下单（网关 /api/orders，卡池自动发货兑换码）。
pub async fn gateway_buy_order(goods_id: i64, sku_id: i64, quantity: i64) -> AppResult<BuyResult> {
    gateway_post("/api/orders", json!({ "goods_id": goods_id, "sku_id": sku_id, "quantity": quantity }))
        .await
}

/// 余额直购下单（稳定商品 key → 网关映射 goods/sku，云端权威扣款发货）。
/// 前端静态商品卡不依赖商品列表回传，仅购买时按 key 下单。
pub async fn gateway_buy_by_key(product_key: &str) -> AppResult<BuyResult> {
    gateway_post("/api/orders", json!({ "product_key": product_key })).await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderCode {
    #[serde(default, alias = "code_id")]
    pub code_id: String,
    #[serde(default)]
    pub code: String,
    #[serde(default)]
    pub status: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShopOrderItem {
    #[serde(default, alias = "order_id")]
    pub order_id: String,
    #[serde(default, alias = "product_name")]
    pub product_name: String,
    #[serde(default)]
    pub amount: String,
    #[serde(default)]
    pub status: String,
    #[serde(default, alias = "created_at")]
    pub created_at: i64,
    #[serde(default)]
    pub codes: Vec<OrderCode>,
}

/// 我的商城订单（含发货兑换码），经网关 /api/user/orders。
pub async fn gateway_orders() -> AppResult<Vec<ShopOrderItem>> {
    #[derive(Deserialize)]
    struct OrdersData {
        #[serde(default)]
        items: Vec<ShopOrderItem>,
    }
    let d: OrdersData = gateway_get("/api/user/orders?page=1&page_size=10").await?;
    Ok(d.items)
}

// ---- 钱包充值（文档 7.7/7.8）：商城充值接口，经网关 wallet/topup/order ----

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopupOrder {
    #[serde(alias = "order_id")]
    pub order_id: String,
    /// 支付中转页（系统浏览器打开扫码/跳转）
    #[serde(alias = "pay_url")]
    pub pay_url: String,
    /// 真二维码内容（weixin:// / alipays:// 等），前端直接渲染
    #[serde(default, alias = "code_url")]
    pub code_url: String,
    #[serde(alias = "expires_at")]
    pub expires_at: i64,
}

/// 充值下单（¥ 实付金额字符串 + 渠道 alipay/wxpay），返回支付中转页。
pub async fn gateway_topup_create(amount_cny: &str, channel: &str) -> AppResult<TopupOrder> {
    gateway_post(
        "/api/wallet/topup/order",
        json!({ "amount": amount_cny, "pay_channel": channel }),
    )
    .await
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TopupStatus {
    #[serde(alias = "order_id")]
    pub order_id: String,
    /// pending / paid / expired
    pub status: String,
    /// ¥ 实付
    pub amount: String,
    #[serde(alias = "paid_at")]
    pub paid_at: Option<i64>,
}

/// 充值订单支付状态轮询。
pub async fn gateway_topup_status(order_id: &str) -> AppResult<TopupStatus> {
    gateway_get(&format!("/api/wallet/topup/order/{}", order_id)).await
}
