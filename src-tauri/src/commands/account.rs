//! 账号命令面（§7.2 account.*）：明文密码只进登录调用，凭据只落钥匙串。

use tauri::Manager;

use crate::ipc::{respond, CommandResult, SimpleAck};
use crate::services::{
    self,
    account::{AccountStatus, KeyItem, LinkStatus, SetupKeyResult},
};

#[tauri::command]
pub async fn account_login(username: String, password: String) -> CommandResult<AccountStatus> {
    respond(services::account::login(username, password).await)
}

#[tauri::command]
pub async fn account_logout(app: tauri::AppHandle) -> CommandResult<SimpleAck> {
    // 先尽力撤销服务端会话（新版 new-api 会话体系：防会话堆积撞 USER_SESSION_ACTIVE_LIMIT）
    if let Some(pat) = crate::credential::get(crate::config::CREDENTIAL_ACCOUNT_TOKEN)
        .ok()
        .flatten()
    {
        let client = crate::newapi::NewApiClient::with_token(crate::config::DEFAULT_BASE_URL, &pat);
        let _ = client.auth_logout().await;
    }
    respond(services::account::logout().map(|_| {
        // SSO 签发的 gp_token 是 24h HttpOnly Cookie，Shop 无法单方面吊销（JWT 无状态）。
        // 退出账号时清空 WebView 浏览数据（含商城 Cookie），保证验收 #6：退出后内嵌商城回落未登录态。
        if let Some(w) = app.get_webview_window(crate::config::MAIN_WINDOW_LABEL) {
            let _ = w.clear_all_browsing_data();
        }
        SimpleAck
    }))
}

#[tauri::command]
pub fn account_status() -> CommandResult<AccountStatus> {
    CommandResult::ok(services::account::current_status())
}

#[tauri::command]
pub async fn account_setup_key() -> CommandResult<SetupKeyResult> {
    respond(services::account::setup_key().await)
}

/// 账号在 new-api 的 key 列表（选择用，不回明文）。
#[tauri::command]
pub async fn account_list_keys() -> CommandResult<Vec<KeyItem>> {
    respond(services::account::list_keys().await)
}

/// 选择某个 key 为当前使用（只选择，不删除）。
#[tauri::command]
pub async fn account_select_key(token_id: i64) -> CommandResult<SetupKeyResult> {
    respond(services::account::select_key(token_id).await)
}

/// 三端联通状态（本机 × new-api × Shop）。
#[tauri::command]
pub async fn account_link_status() -> CommandResult<LinkStatus> {
    respond(services::account::link_status().await)
}

/// 发送邮箱验证码（站点 SMTP）。
#[tauri::command]
pub async fn account_send_code(email: String) -> CommandResult<SimpleAck> {
    respond(services::account::send_code(email).await.map(|_| SimpleAck))
}

/// 注册即就绪（邮箱即账号）：注册 → 登录 → 自动建 key。用户名自动生成。
#[tauri::command]
pub async fn account_register(
    email: String,
    password: String,
    code: String,
) -> CommandResult<AccountStatus> {
    respond(services::account::register(email, password, code).await)
}

/// 发送密码重置邮件。
#[tauri::command]
pub async fn account_send_password_reset(email: String) -> CommandResult<SimpleAck> {
    respond(services::account::send_password_reset(email).await.map(|_| SimpleAck))
}

/// 重置密码（粘贴邮件链接中的 email+token）→ 自动登录，返回一次性新密码。
#[tauri::command]
pub async fn account_reset_password(
    email: String,
    token: String,
) -> CommandResult<crate::services::account::ResetPasswordResult> {
    respond(services::account::reset_password(email, token).await)
}
