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
    // 尽力撤销网关会话（网关 24h 会话，撤销失败不阻断本地登出）
    let _ = crate::services::shop::gateway_logout().await;
    respond(services::account::logout().map(|_| {
        // 退出账号时清空 WebView 浏览数据（含商城 Cookie），保证退出后回落未登录态。
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

/// 绑定中转站账号（SSO 失败引导，契约 4.2）：仅中转站已有该账号时使用。
#[tauri::command]
pub async fn account_bind_newapi(username: String, password: String) -> CommandResult<SimpleAck> {
    respond(
        services::shop::bind_newapi(username, password)
            .await
            .map(|_| SimpleAck),
    )
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

/// 发送密码重置验证码（走网关）。
#[tauri::command]
pub async fn account_send_password_reset(email: String) -> CommandResult<SimpleAck> {
    respond(services::account::send_password_reset(email).await.map(|_| SimpleAck))
}

/// 重置密码（验证码 + 新密码）→ 自动登录，返回登录态。
#[tauri::command]
pub async fn account_reset_password(
    email: String,
    code: String,
    new_password: String,
) -> CommandResult<AccountStatus> {
    respond(services::account::reset_password(email, code, new_password).await)
}
