//! 账号编排：登录/登出/状态/自动建 key。凭据只进钥匙串，明文不出 Rust 侧。

use serde::Serialize;

use crate::config;
use crate::credential;
use crate::error::{AppError, AppResult};
use crate::newapi::NewApiClient;

/// 自动创建的令牌名（复用判定依据：同名且启用的令牌直接取密钥，绝不重复建号）。
pub const TOKEN_NAME: &str = "绿池自动生成";

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountStatus {
    pub logged_in: bool,
    pub username: String,
    pub user_id: i64,
    pub has_api_key: bool,
    /// 脱敏后的密钥（sk-****xx），供界面展示；明文绝不出 Rust 侧。
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
    let logged_in = opt(config::CREDENTIAL_ACCOUNT_TOKEN).is_some();
    let username = opt(config::CREDENTIAL_ACCOUNT_USERNAME).unwrap_or_default();
    let user_id = opt(config::CREDENTIAL_ACCOUNT_USER_ID)
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);
    let masked_api_key = opt(config::CREDENTIAL_ACCOUNT_API_KEY).map(|k| mask(&k));
    let has_api_key = masked_api_key.is_some();
    AccountStatus { logged_in, username, user_id, has_api_key, masked_api_key }
}

pub fn require_token() -> AppResult<String> {
    opt(config::CREDENTIAL_ACCOUNT_TOKEN).ok_or_else(|| AppError::Other("请先登录".into()))
}

pub fn require_api_key() -> AppResult<String> {
    opt(config::CREDENTIAL_ACCOUNT_API_KEY).ok_or_else(|| AppError::Other("请先完成一键配置".into()))
}

pub async fn login(identifier: String, password: String) -> AppResult<AccountStatus> {
    let identifier = identifier.trim().to_string();
    if identifier.is_empty() || password.is_empty() {
        return Err(AppError::Other("请填写邮箱和密码".into()));
    }
    let client = NewApiClient::default_client();
    // 三端（工具 / Shop / new-api）username = 完整邮箱，完全一致；直接用邮箱登录
    let data = client.login(&identifier, &password).await?;
    if data.access_token.is_empty() {
        return Err(AppError::Other("登录响应缺少令牌".into()));
    }
    let user_id = data.user.as_ref().map(|u| u.id).unwrap_or(0);
    // 登录响应已带 username/email（buildSelfUserData），不再多打一次 self_info（省 1 次往返）
    let display_name = data
        .user
        .as_ref()
        .map(|u| u.username.clone())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| identifier.clone());
    let account_email = data
        .user
        .as_ref()
        .map(|u| u.email.clone())
        .filter(|s| !s.is_empty());
    // 换号登录：清掉上一账号的密钥与选择，避免跨账号串钥。
    // ⚠ 同账号重登（含 ensure_session 静默续期）必须保留密钥，否则会话过期即丢选择。
    let same_account = opt(config::CREDENTIAL_ACCOUNT_USERNAME)
        .map(|old| old == display_name)
        .unwrap_or(false);
    if !same_account {
        let _ = credential::delete(config::CREDENTIAL_ACCOUNT_API_KEY);
        let _ = credential::delete(config::CREDENTIAL_ACCOUNT_KEY_TOKEN_ID);
    }
    credential::set(config::CREDENTIAL_ACCOUNT_TOKEN, &data.access_token)?;
    credential::set(config::CREDENTIAL_ACCOUNT_USERNAME, &display_name)?;
    credential::set(config::CREDENTIAL_ACCOUNT_USER_ID, &user_id.to_string())?;
    // 邮箱落钥匙串（绑定 Shop 用）
    if let Some(email) = &account_email {
        credential::set(config::CREDENTIAL_ACCOUNT_EMAIL, email)?;
    }
    // 会话令牌会过期（Gen B 带 access_expires_at）：密码入钥匙串，供失效时静默重登（§13：凭据只进钥匙串）
    credential::set(config::CREDENTIAL_ACCOUNT_PASSWORD, &password)?;
    // Shop 软轨改为后台非阻塞：不拖慢登录主路径；结果由总览刷新时探测。
    // Shop 账号体系是邮箱，优先用邮箱而非用户名；新邮箱未在 Shop 注册时失败属预期。
    let shop_identifier = account_email.clone().unwrap_or_else(|| identifier.clone());
    let shop_pw = password.clone();
    tokio::spawn(async move {
        let _ = crate::services::shop::client_login(&shop_identifier, &shop_pw).await;
    });
    Ok(current_status())
}

/// 注册发码：走 Shop SMTP（scene=register）。用户收一个码，Shop 注册时后端同步在 new-api 建号。
pub async fn send_code(email: String) -> AppResult<()> {
    let email = email.trim().to_string();
    crate::services::shop::send_register_code(&email).await
}

/// 发送密码重置邮件（防枚举：无论邮箱是否存在都成功；前端提示"若存在会收到邮件"）。
pub async fn send_password_reset(email: String) -> AppResult<()> {
    let email = email.trim().to_string();
    let client = NewApiClient::default_client();
    client.send_password_reset(&email).await
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ResetPasswordResult {
    /// 站点生成的新密码（仅此一次返回，界面提示用户保存）。
    pub new_password: String,
}

/// 重置密码：粘贴邮件里的重置链接解析出的 email+token → 站点生成新密码 → 自动登录。
pub async fn reset_password(email: String, token: String) -> AppResult<ResetPasswordResult> {
    let email = email.trim().to_string();
    let token = token.trim().to_string();
    if email.is_empty() || token.is_empty() {
        return Err(AppError::Other("请先粘贴邮件中的重置链接".into()));
    }
    let client = NewApiClient::default_client();
    let new_password = client.reset_password(&email, &token).await?;
    if new_password.is_empty() {
        return Err(AppError::Other("重置链接无效或已过期".into()));
    }
    let _ = login(email, new_password.clone()).await?;
    Ok(ResetPasswordResult { new_password })
}

/// 邮箱即账号：注册只需 邮箱+验证码+密码。
/// 用户名由 @ 前缀自动生成（撞名自动加随机尾缀，用户全程无感，登录用邮箱）。
/// 注册 → 登录 → 自动建 key；密钥生成失败不影响账号，可稍后重试。
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

    // Shop 注册（后端同步在 new-api 建号：admin create 免邮箱验证码）。
    // 失败原样上抛（邀请码错 / 验证码错 / 邮箱已注册）。
    crate::services::shop::register(&email, code.trim(), &password).await?;
    // new-api 账号已由 Shop 后端建好（username = 邮箱前缀）→ 登录 + 建 key
    let _ = login(email, password).await?;
    let _ = setup_key().await;
    Ok(current_status())
}

pub fn logout() -> AppResult<()> {
    for k in [
        config::CREDENTIAL_ACCOUNT_TOKEN,
        config::CREDENTIAL_ACCOUNT_USERNAME,
        config::CREDENTIAL_ACCOUNT_USER_ID,
        config::CREDENTIAL_ACCOUNT_API_KEY,
        config::CREDENTIAL_ACCOUNT_KEY_TOKEN_ID,
        config::CREDENTIAL_ACCOUNT_PASSWORD,
        config::CREDENTIAL_ACCOUNT_EMAIL,
        config::SHOP_PASSWORD,
    ] {
        let _ = credential::delete(k);
    }
    let _ = crate::services::shop::logout();
    Ok(())
}

/// new-api 会话失效识别（中英文，含 401 原文与人话翻译文案）。
/// 注意：这是兜底路径——主判定在 jwt_expired（结构化），此处只捕漏网之鱼。
fn is_session_expired(msg: &str) -> bool {
    let m = msg.to_lowercase();
    m.contains("unauthorized")
        || m.contains("not logged in")
        || m.contains("no access token")
        || m.contains("无权")
        || m.contains("未登录")
        || m.contains("登录已过期")
        || m.contains("已过期")
        || m.contains("已失效")
        || m.contains("重新登录")
        || m.contains("token expired")
        || m.contains("session revoked")
        || m.contains("auth_token_expired")
        || m.contains("auth_session_revoked")
}

/// JWT exp 是否已过（留 30s 余量）。解析失败按未过期处理，交给服务端判定。
fn jwt_expired(pat: &str) -> bool {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    match crate::newapi::client::jwt_exp(pat) {
        Some(exp) => now >= exp - 30,
        None => false,
    }
}

/// 尝试用 refresh cookie 轮换会话（服务端 rotate 同一会话，不新建）。
/// 成功返回新 access token 并写回钥匙串；任何失败返回 None（调用方回退重登）。
/// 这是根治「会话数堆积（50 上限）」的关键路径：JWT 过期不再新建会话。
async fn try_refresh() -> Option<String> {
    if opt(config::CREDENTIAL_ACCOUNT_TOKEN).is_none() {
        return None; // 无存量 token（未登录/登出），refresh 无意义
    }
    let client = NewApiClient::default_client();
    match client.refresh_auth().await {
        Ok(new_token) if !new_token.is_empty() => {
            let _ = credential::set(config::CREDENTIAL_ACCOUNT_TOKEN, &new_token);
            Some(new_token)
        }
        _ => None,
    }
}

/// 拿一个可用的 new-api 会话令牌：
/// 1) 存量令牌 JWT 未过期 → 直接用（服务端判失效再走续期）；
/// 2) JWT 已过期或服务端报失效 → **优先 refresh**（轮换同一会话，不新建，根治会话堆积）；
/// 3) refresh 不可用（无 cookie/会话已吊销）→ 才用钥匙串密码静默重登换新。
/// 无法恢复时返回 SessionExpired。
pub async fn ensure_session() -> AppResult<String> {
    if let Some(pat) = opt(config::CREDENTIAL_ACCOUNT_TOKEN) {
        if !jwt_expired(&pat) {
            let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &pat);
            match client.self_info().await {
                Ok(info) => {
                    // 旧会话首次补齐邮箱（绑定 Shop 需要）；已缓存则跳过，避免每次操作写钥匙串
                    if !info.email.is_empty() && credential::get(config::CREDENTIAL_ACCOUNT_EMAIL).ok().flatten().is_none() {
                        let _ = credential::set(config::CREDENTIAL_ACCOUNT_EMAIL, &info.email);
                    }
                    return Ok(pat);
                }
                Err(e) => {
                    if !is_session_expired(&e.to_string()) {
                        return Err(e); // 网络/站点错误原样上抛，不误判成过期
                    }
                    // 服务端判定失效 → 走续期
                }
            }
        }
        // JWT 已过期（或服务端失效）→ refresh 优先（不新建会话），失败才重登
        if let Some(t) = try_refresh().await {
            return Ok(t);
        }
    }
    let identifier = opt(config::CREDENTIAL_ACCOUNT_USERNAME).ok_or(AppError::SessionExpired)?;
    let password = opt(config::CREDENTIAL_ACCOUNT_PASSWORD).ok_or(AppError::SessionExpired)?;
    login(identifier, password).await?;
    opt(config::CREDENTIAL_ACCOUNT_TOKEN).ok_or(AppError::SessionExpired)
}

/// 建密钥（自动确保）：优先复用同名启用令牌 → 其次任一启用令牌 → 都没有才创建。
/// 新账号自动生成一个新 key 并默认选中；不提供删除。
pub async fn setup_key() -> AppResult<SetupKeyResult> {
    let pat = ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &pat);

    let items = client.list_tokens().await.unwrap_or_default();
    let chosen = items
        .iter()
        .find(|t| t.status == 1 && t.name == TOKEN_NAME)
        .or_else(|| items.iter().find(|t| t.status == 1))
        .map(|t| t.id);

    let token_id = match chosen {
        Some(id) => id,
        None => {
            client.create_token(TOKEN_NAME).await?;
            let items = client.list_tokens().await.unwrap_or_default();
            items
                .iter()
                .find(|t| t.name == TOKEN_NAME && t.status == 1)
                .map(|t| t.id)
                .ok_or_else(|| AppError::Other("令牌已创建但未能定位，请到站点令牌页查看".into()))?
        }
    };

    let key = client.get_token_key(token_id).await?;
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

/// 账号在 new-api 的 key 列表（含当前选中标记）。绝不返回明文。
pub async fn list_keys() -> AppResult<Vec<KeyItem>> {
    let pat = ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &pat);
    let selected = stored_token_id();
    Ok(client
        .list_tokens()
        .await?
        .into_iter()
        .map(|t| KeyItem { token_id: t.id, name: t.name, status: t.status, selected: selected == Some(t.id) })
        .collect())
}

/// 选择某个 key 作为当前使用：取明文存钥匙串。只选择，不删除。
pub async fn select_key(token_id: i64) -> AppResult<SetupKeyResult> {
    let pat = ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &pat);
    let key = client.get_token_key(token_id).await?;
    persist_key(&key, token_id)
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkStatus {
    pub logged_in: bool,
    /// new-api 端账号联通（PAT 有效且能取到 self）。
    pub newapi_ok: bool,
    pub newapi_username: String,
    /// Shop 端账号关联（Shop 未上线前恒为未关联）。
    pub shop_ready: bool,
    pub shop_linked: bool,
    /// Shop 自动关联失败的具体原因（成功时为 None；界面显示给人看）。
    pub shop_error: Option<String>,
    pub has_key: bool,
}

/// 三端联通状态：本机登录 × new-api 关联 × Shop 关联（真实探测）。会话失效会先尝试静默续期。
pub async fn link_status() -> AppResult<LinkStatus> {
    let s = current_status();
    let shop = crate::services::shop::status().await;
    let mut newapi_ok = false;
    let mut newapi_username = String::new();
    if ensure_session().await.is_ok() {
        // ensure_session 内部已用 self_info 校验过令牌；不再重复打一次（省 1 次往返）
        newapi_ok = true;
        newapi_username = opt(config::CREDENTIAL_ACCOUNT_USERNAME).unwrap_or_default();
    }
    // 自动补关联：Shop 已上线但未登录态时，用账号邮箱+密码静默登录一次；
    // 失败原因记录下来显示（不再静默吞掉——这是此前"未关联却查不出原因"的教训）
    let mut shop_linked = shop.ready && crate::services::shop::linked();
    let mut shop_error: Option<String> = None;
    if shop.ready && !shop_linked && s.logged_in {
        if let (Some(email), Some(password)) = (
            credential::get(config::CREDENTIAL_ACCOUNT_EMAIL).ok().flatten(),
            crate::services::shop::stored_shop_password(),
        ) {
            match crate::services::shop::client_login(&email, &password).await {
                Ok(_) => shop_linked = true,
                Err(e) => shop_error = Some(e.to_string()),
            }
        }
    }
    Ok(LinkStatus {
        logged_in: s.logged_in,
        newapi_ok,
        newapi_username,
        shop_ready: shop.ready,
        shop_linked,
        shop_error,
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

    /// 回归锁：人话翻译后的错误文案必须仍被识别为「会话失效」，
    /// 否则静默重登不会触发（本次线上事故的根因）。
    #[test]
    fn session_expired_matches_humanized_messages() {
        assert!(is_session_expired("登录状态已失效，请重新登录"));
        assert!(is_session_expired("unauthorized"));
        assert!(is_session_expired("无权进行此操作，未登录"));
        // 非失效类错误不得误判
        assert!(!is_session_expired("登录会话数已达上限：请在已登录的网页端撤销其他会话"));
        assert!(!is_session_expired("登录过于频繁：服务器限流中，请稍后再试"));
        assert!(!is_session_expired("网络错误"));
    }

    /// JWT 过期判定：exp=1 的令牌必然已过；解析失败不误判。
    #[test]
    fn jwt_expired_detects_stale_token() {
        assert!(jwt_expired("a.eyJleHAiOjF9.c")); // exp=1
        assert!(!jwt_expired("garbage")); // 解析失败 → 交给服务端判定
    }
}
