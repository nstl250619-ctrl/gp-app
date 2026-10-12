//! 商城命令面（§7.2 shop.* / redeem.*）。Shop 已上线（2026-10-07），状态与钱包走真实接口。

use crate::ipc::{respond, CommandResult, SimpleAck};
use crate::services::{
    self,
    shop::{
        BuyResult, ProductDetail, ProductItem, RedeemResult, ShopOrderItem, ShopStatus,
        WalletSummary,
    },
};

/// 我的商城订单（含发货兑换码）。
#[tauri::command]
pub async fn shop_my_orders() -> CommandResult<Vec<ShopOrderItem>> {
    respond(services::shop::gateway_orders().await)
}

/// 商城商品列表（网关 /api/products，Shop 在售商品）。
#[tauri::command]
pub async fn shop_products() -> CommandResult<Vec<ProductItem>> {
    respond(services::shop::gateway_products().await)
}

/// 商品详情（含 sku）。
#[tauri::command]
pub async fn shop_product_detail(id: i64) -> CommandResult<ProductDetail> {
    respond(services::shop::gateway_product_detail(id).await)
}

/// 余额直购下单（稳定商品 key → 网关映射，卡池自动发货兑换码）。
#[tauri::command]
pub async fn shop_buy_order(product_key: String) -> CommandResult<BuyResult> {
    respond(services::shop::gateway_buy_by_key(&product_key).await)
}

/// 充值下单（¥ 实付 + 支付渠道 alipay/wxpay），返回支付中转页。
#[tauri::command]
pub async fn shop_topup_create(amount_cny: String, channel: String) -> CommandResult<crate::services::shop::TopupOrder> {
    respond(services::shop::gateway_topup_create(&amount_cny, &channel).await)
}

/// 充值订单支付状态轮询。
#[tauri::command]
pub async fn shop_topup_status(order_id: String) -> CommandResult<crate::services::shop::TopupStatus> {
    respond(services::shop::gateway_topup_status(&order_id).await)
}

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
