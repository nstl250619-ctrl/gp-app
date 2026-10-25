//! 账号编排（P2 收敛）：登录/登出/状态/自动建 key 统一走网关（Shop 网关代理 new-api）。
//! 凭据只进钥匙串；登录态 = Shop 会话 token（gp_token），不存 new-api access_token。

use serde::Serialize;

use crate::config;
use crate::credential;
use crate::error::{AppError, AppResult};
use crate::services::shop;

/// 自动创建的令牌名（复用判定依据：同名且启用的令牌直接取密钥，绝不重复建号）。
pub const TOKEN_NAME: &str = "绿池自动生成";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub logged_in: bool,
    pub username: String,
    pub user_id: i64,
    pub has_api_key: bool,
    #[serde(default)]
    pub masked_api_key: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupKeyResult {
    pub token_name: String,
    pub masked_key: String,
}

fn opt(account: &str) -> Option<String> {
    credential::get(account).unwrap_or(None)
}

pub fn current_status() -> AccountStatus {
    // 登录态 = Shop 会话 token（网关 gp_token）
    let logged_in = opt(config::SHOP_AUTH_TOKEN).is_some();
    let username = opt(config::CREDENTIAL_ACCOUNT_USERNAME).unwrap_or_default();
    let user_id = opt(config::SHOP_USER_ID).and_then(|s| s.parse().ok()).unwrap_or(0);
    let masked_api_key = opt(config::CREDENTIAL_ACCOUNT_API_KEY).map(|k| mask(&k));
    let has_api_key = masked_api_key.is_some();
    AccountStatus { logged_in, username, user_id, has_api_key, masked_api_key }
}

pub fn require_token() -> AppResult<String> {
    opt(config::SHOP_AUTH_TOKEN).ok_or_else(|| AppError::Other("请先登录".into()))
}

pub fn require_api_key() -> AppResult<String> {
    opt(config::CREDENTIAL_ACCOUNT_API_KEY).ok_or_else(|| AppError::Other("请先完成一键配置".into()))
}

/// 登录：走网关（Shop 网关登录，云端同步建立 new-api 会话）。
pub async fn login(identifier: String, password: String) -> AppResult<AccountStatus> {
    let identifier = identifier.trim().to_string();
    if identifier.is_empty() || password.is_empty() {
        return Err(AppError::Other("请填写邮箱和密码".into()));
    }
    let user = shop::client_login(&identifier, &password).await?;
    // 用户主动登录成功：清除静默重登的失败退避（立即恢复会话能力，无需等 60 秒）
    reset_session_backoff();
    // 换号登录：清掉上一账号的密钥与选择，避免跨账号串钥。
    let same_account = opt(config::CREDENTIAL_ACCOUNT_USERNAME)
        .map(|old| old == user.username || old == user.email)
        .unwrap_or(false);
    if !same_account {
        let _ = credential::delete(config::CREDENTIAL_ACCOUNT_API_KEY);
        let _ = credential::delete(config::CREDENTIAL_ACCOUNT_KEY_TOKEN_ID);
        let _ = credential::delete(config::CREDENTIAL_DSH_API_KEY);
        // 换号：清分组缓存，防上一个账号的 svip 身份串到新账号
        crate::commands::app::reset_group_cache();
    }
    // 网关登录已存 SHOP_AUTH_TOKEN / SHOP_USER_ID；补齐账号信息 + 密码供静默重登。
    credential::set(config::CREDENTIAL_ACCOUNT_USERNAME, &user.username)?;
    if !user.email.is_empty() {
        credential::set(config::CREDENTIAL_ACCOUNT_EMAIL, &user.email)?;
    }
    credential::set(config::CREDENTIAL_ACCOUNT_PASSWORD, &password)?;
    // 登录后自动确保密钥就绪（幂等：有同名 key 直接取明文，无则自动创建）。
    // 旧版靠已移除的总览页「生成密钥」兜底；失败不阻断登录（配置页会再兜底重试）。
    let _ = setup_key().await;
    let _ = setup_dsh_key().await;
    Ok(current_status())
}

/// 注册发码：走 Shop SMTP（网关），后端同步在 new-api 建号。
pub async fn send_code(email: String) -> AppResult<()> {
    let email = email.trim().to_string();
    shop::send_register_code(&email).await
}

/// 发送密码重置验证码（走网关，防枚举）。Shop SMTP 发码到邮箱。
pub async fn send_password_reset(email: String) -> AppResult<()> {
    let email = email.trim().to_string();
    shop::send_reset_code(&email).await
}

/// 找回密码重置：验证码 + 新密码（走网关，改 Shop 密码并同步 new-api 密码）→ 自动登录。
pub async fn reset_password(email: String, code: String, new_password: String) -> AppResult<AccountStatus> {
    let email = email.trim().to_string();
    let code = code.trim().to_string();
    if email.is_empty() || code.is_empty() {
        return Err(AppError::Other("请先获取邮箱验证码".into()));
    }
    if new_password.chars().count() < 8 {
        return Err(AppError::Other("密码至少 8 位".into()));
    }
    shop::reset_password(&email, &code, &new_password).await?;
    // 重置成功后用新密码登录（网关），返回登录态
    login(email, new_password).await
}

/// 注册：走网关（Shop 注册 → 后端同步建 new-api 号）→ 自动登录 → 建 key。
pub async fn register(email: String, password: String, code: String) -> AppResult<AccountStatus> {
    let email = email.trim().to_string();
    if !email.contains('@') || email.starts_with('@') || email.ends_with('@') || email.chars().count() > 50 {
        return Err(AppError::Other("邮箱格式不正确".into()));
    }
    if password.chars().count() < 8 {
        return Err(AppError::Other("密码至少 8 位".into()));
    }
    if code.trim().is_empty() {
        return Err(AppError::Other("请填写邮箱验证码".into()));
    }
    shop::register(&email, code.trim(), &password).await?;
    let _ = login(email, password).await?;
    let _ = setup_key().await;
    let _ = setup_dsh_key().await;
    Ok(current_status())
}

pub fn logout() -> AppResult<()> {
    for k in [
        config::SHOP_AUTH_TOKEN,
        config::SHOP_USER_ID,
        config::CREDENTIAL_ACCOUNT_TOKEN,
        config::CREDENTIAL_ACCOUNT_USERNAME,
        config::CREDENTIAL_ACCOUNT_USER_ID,
        config::CREDENTIAL_ACCOUNT_API_KEY,
        config::CREDENTIAL_ACCOUNT_KEY_TOKEN_ID,
        config::CREDENTIAL_DSH_API_KEY,
        config::CREDENTIAL_ACCOUNT_PASSWORD,
        config::CREDENTIAL_ACCOUNT_EMAIL,
        config::SHOP_PASSWORD,
    ] {
        let _ = credential::delete(k);
    }
    // 登出：清分组缓存（防跨账号串味）
    crate::commands::app::reset_group_cache();
    let _ = shop::logout();
    Ok(())
}

/// 会话建立失败退避（避免撞 Shop 登录限流：同账号 10 次/分钟）。
static SESSION_FAIL_AT: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);

/// 清除会话失败退避（用户主动登录成功 / 会话恢复后调用）。
pub fn reset_session_backoff() {
    if let Ok(mut c) = SESSION_FAIL_AT.lock() {
        *c = None;
    }
}

/// 确保登录态（网关会话）：失效时用密码静默重登一次。
/// 重登失败后 60 秒内直接返回 SessionExpired——避免 app.status 等多处调用反复打登录接口，
/// 把 Shop 的「同账号 10 次/分钟」限流打满（用户会看到「操作过于频繁」）。
pub async fn ensure_session() -> AppResult<String> {
    if let Ok(c) = SESSION_FAIL_AT.lock() {
        if let Some(t) = *c {
            if t.elapsed() < std::time::Duration::from_secs(60) {
                return Err(AppError::SessionExpired);
            }
        }
    }
    if let Some(t) = opt(config::SHOP_AUTH_TOKEN) {
        // 探活：能取到 self 即会话有效
        if shop::gateway_self().await.is_ok() {
            reset_session_backoff();
            return Ok(t);
        }
    }
    let identifier = opt(config::CREDENTIAL_ACCOUNT_EMAIL)
        .or_else(|| opt(config::CREDENTIAL_ACCOUNT_USERNAME))
        .ok_or(AppError::SessionExpired)?;
    let password = shop::stored_shop_password().ok_or(AppError::SessionExpired)?;
    match shop::client_login(&identifier, &password).await {
        Ok(_) => {
            reset_session_backoff();
            opt(config::SHOP_AUTH_TOKEN).ok_or(AppError::SessionExpired)
        }
        Err(e) => {
            if let Ok(mut c) = SESSION_FAIL_AT.lock() {
                *c = Some(std::time::Instant::now());
            }
            Err(e)
        }
    }
}

/// 建密钥（自动确保）：优先复用同名启用令牌 → 任一启用令牌 → 新建。走网关。
pub async fn setup_key() -> AppResult<SetupKeyResult> {
    let _ = ensure_session().await?;
    let items = shop::gateway_list_keys().await.unwrap_or_default();
    let chosen = items
        .iter()
        .find(|t| t.status == 1 && t.name == TOKEN_NAME)
        .or_else(|| items.iter().find(|t| t.status == 1))
        .map(|t| t.id);

    let token_id = match chosen {
        Some(id) => id,
        None => {
            shop::gateway_create_key(TOKEN_NAME, None).await?;
            let items = shop::gateway_list_keys().await.unwrap_or_default();
            items
                .iter()
                .find(|t| t.name == TOKEN_NAME && t.status == 1)
                .map(|t| t.id)
                .ok_or_else(|| AppError::Other("令牌已创建但未能定位，请到站点令牌页查看".into()))?
        }
    };

    let key = shop::gateway_key_plain(token_id).await?;
    persist_key(&key, token_id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyItem {
    pub token_id: i64,
    pub name: String,
    pub status: i64,
    pub selected: bool,
}

fn stored_token_id() -> Option<i64> {
    opt(config::CREDENTIAL_ACCOUNT_KEY_TOKEN_ID).and_then(|s| s.parse().ok())
}

fn persist_key(key: &str, token_id: i64) -> AppResult<SetupKeyResult> {
    credential::set(config::CREDENTIAL_ACCOUNT_API_KEY, key)?;
    credential::set(config::CREDENTIAL_ACCOUNT_KEY_TOKEN_ID, &token_id.to_string())?;
    Ok(SetupKeyResult { token_name: String::new(), masked_key: mask(key) })
}

/// 建 DSH 专属 key（幂等）：复用同名启用令牌 → 新建（group=DSH-Only）→ 取明文存独立钥匙串条目。
/// 失败不阻断登录/注册（与 setup_key 同策略）；DSH 卡导入时若无专属 key，C3 会回退通用 key。
pub async fn setup_dsh_key() -> AppResult<SetupKeyResult> {
    let _ = ensure_session().await?;
    let items = shop::gateway_list_keys().await.unwrap_or_default();
    let existing = items.iter().find(|t| t.status == 1 && t.name == config::DSH_TOKEN_NAME);
    let token_id = match existing {
        // V4：复用同名令牌时校验分组——分组非空且不匹配则报错，防「名字对但分组错」的令牌被误用。
        Some(t) if !t.group.is_empty() && t.group != config::DSH_GROUP => {
            return Err(AppError::Other(format!(
                "已存在同名令牌「{}」但分组为「{}」，请到站点令牌页将其分组改为「{}」",
                t.name, t.group, config::DSH_GROUP
            )));
        }
        Some(t) => t.id,
        None => {
            shop::gateway_create_key(config::DSH_TOKEN_NAME, Some(config::DSH_GROUP)).await?;
            let items = shop::gateway_list_keys().await.unwrap_or_default();
            items
                .iter()
                .find(|t| t.name == config::DSH_TOKEN_NAME && t.status == 1)
                .map(|t| t.id)
                .ok_or_else(|| AppError::Other("DSH 专属令牌已创建但未能定位，请到站点令牌页查看".into()))?
        }
    };
    let key = shop::gateway_key_plain(token_id).await?;
    // 独立钥匙串条目（与通用 key 分离），C3 写入时按 target==dsh 读取
    credential::set(config::CREDENTIAL_DSH_API_KEY, &key)?;
    Ok(SetupKeyResult { token_name: config::DSH_TOKEN_NAME.into(), masked_key: mask(&key) })
}

/// 账号在网关的 key 列表（含选中标记），走网关。绝不返回明文。
pub async fn list_keys() -> AppResult<Vec<KeyItem>> {
    let _ = ensure_session().await?;
    let selected = stored_token_id();
    Ok(shop::gateway_list_keys()
        .await?
        .into_iter()
        .map(|t| KeyItem { token_id: t.id, name: t.name, status: t.status, selected: selected == Some(t.id) })
        .collect())
}

/// current_key_group 结果缓存（30 秒，按 token_id 区分）：切卡/刷新时避免重复拉 keys + self 两次网络请求。
static KEY_GROUP_CACHE: std::sync::Mutex<Option<(std::time::Instant, i64, Option<String>)>> =
    std::sync::Mutex::new(None);

/// 当前选中 key 的实际分组（模型列表过滤用）：key.group 非空用之，空则回退账号分组
/// （与 new-api 鉴权语义一致：空 group 的令牌按 user.group 路由）。拿不到时返回 None（列表退回账号级并集）。
pub async fn current_key_group() -> Option<String> {
    let token_id = stored_token_id()?;
    // 30 秒缓存：模型列表刷新频繁，避免每次切卡都打两次网关（keys + self）
    if let Ok(c) = KEY_GROUP_CACHE.lock() {
        if let Some((at, tid, g)) = c.as_ref() {
            if *tid == token_id && at.elapsed() < std::time::Duration::from_secs(30) {
                return g.clone();
            }
        }
    }
    let g = async {
        let items = shop::gateway_list_keys().await.ok()?;
        let g = items.iter().find(|t| t.id == token_id)?.group.clone();
        if !g.is_empty() {
            return Some(g);
        }
        let self_data = shop::gateway_self().await.ok()?;
        Some(self_data.group)
    }
    .await;
    if let Ok(mut c) = KEY_GROUP_CACHE.lock() {
        *c = Some((std::time::Instant::now(), token_id, g.clone()));
    }
    g
}

/// 选择某个 key：取明文存钥匙串。走网关。
pub async fn select_key(token_id: i64) -> AppResult<SetupKeyResult> {
    let _ = ensure_session().await?;
    let key = shop::gateway_key_plain(token_id).await?;
    persist_key(&key, token_id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    pub logged_in: bool,
    pub newapi_ok: bool,
    pub newapi_username: String,
    pub shop_ready: bool,
    pub shop_linked: bool,
    pub shop_error: Option<String>,
    pub has_key: bool,
}

/// 三端联通状态（P2 收敛后：网关登录即三端绑定，一次探测）。
pub async fn link_status() -> AppResult<LinkStatus> {
    let s = current_status();
    let (newapi_ok, newapi_username) = match ensure_session().await {
        Ok(_) => (true, opt(config::CREDENTIAL_ACCOUNT_USERNAME).unwrap_or_default()),
        Err(_) => (false, String::new()),
    };
    Ok(LinkStatus {
        logged_in: s.logged_in,
        newapi_ok,
        newapi_username,
        shop_ready: true,
        shop_linked: s.logged_in,
        shop_error: None,
        has_key: s.has_api_key,
    })
}

/// 脱敏：前 3 后 2（§13.2）。不在本模块之外输出明文。
pub fn mask(secret: &str) -> String {
    let chars: Vec<char> = secret.chars().collect();
    if chars.len() <= 8 {
        return "****".to_string();
    }
    let head: String = chars[..3].iter().collect();
    let tail: String = chars[chars.len() - 2..].iter().collect();
    format!("{}****{}", head, tail)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mask_hides_middle() {
        let m = mask("sk-abcdefghijklmn");
        assert!(m.starts_with("sk-"));
        assert!(m.ends_with("mn"));
        assert!(!m.contains("abcdefgh"));
    }

    #[test]
    fn mask_short_secret() {
        assert_eq!(mask("short"), "****");
    }
}
