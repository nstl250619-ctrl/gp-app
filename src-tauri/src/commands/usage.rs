//! 用量命令面（§7.2 usage.*）。

use crate::ipc::{respond, CommandResult};
use crate::services;
use crate::services::usage::QuotaSummary;

#[tauri::command]
pub async fn usage_get_quota() -> CommandResult<QuotaSummary> {
    respond(services::usage::get_quota().await)
}

/// 最近充值记录（站点侧，脱敏不做——money/trade_no 本就非敏感）。
#[tauri::command]
pub async fn usage_topups() -> CommandResult<Vec<crate::newapi::TopUpRecord>> {
    respond(services::usage::topups().await)
}

/// 兑换记录（logs type=1：兑换码核销不写 top_ups 表，从日志取）。
#[tauri::command]
pub async fn usage_redemption_records() -> CommandResult<Vec<crate::services::usage::RedemptionRecord>> {
    respond(services::usage::redemption_records().await)
}

/// 用量明细 + 聚合（时间范围由前端计算 Unix 秒）。
#[tauri::command]
pub async fn usage_detail(start_ts: i64, end_ts: i64) -> CommandResult<crate::services::usage::UsageDetail> {
    respond(services::usage::detail(start_ts, end_ts).await)
}
