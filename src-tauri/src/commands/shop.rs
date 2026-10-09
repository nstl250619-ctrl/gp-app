//! 商城命令面（§7.2 shop.* / redeem.*）。Shop 已上线（2026-10-07），状态与钱包走真实接口。

use crate::ipc::{respond, CommandResult, SimpleAck};
use crate::services::{self, shop::{RedeemResult, ShopStatus, WalletSummary}};

/// 商城状态（真实探测：能建立会话即已上线）。
#[tauri::command]
pub async fn shop_status() -> CommandResult<ShopStatus> {
    CommandResult::ok(services::shop::status().await)
}

/// Shop 钱包（7.5：余额 USD + 折算率）。
#[tauri::command]
pub async fn shop_wallet() -> CommandResult<WalletSummary> {
    respond(services::shop::wallet().await)
}

/// Shop SMTP 发码（老账号绑定商城用）。
#[tauri::command]
pub async fn shop_send_bind_code() -> CommandResult<SimpleAck> {
    respond(services::shop::send_bind_code().await.map(|_| SimpleAck))
}

/// 绑定 Shop 账号（当前账号邮箱 + 密码在商城侧建号并关联；密码不同时可单独指定）。
#[tauri::command]
pub async fn shop_bind(code: String, shop_password: Option<String>) -> CommandResult<WalletSummary> {
    respond(services::shop::bind(code, shop_password).await)
}

/// 生成 SSO 内嵌地址（每次重铸新 token，兼容一次性模式）。
#[tauri::command]
pub async fn shop_sso_url() -> CommandResult<String> {
    respond(services::shop::sso_url().await)
}

#[tauri::command]
pub async fn redeem_redeem(code: String) -> CommandResult<RedeemResult> {
    respond(services::shop::redeem(code).await)
}
