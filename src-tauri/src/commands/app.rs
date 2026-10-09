//! 应用状态与系统命令面（§7.2 app.* / system.*）。

use serde::Serialize;
use tauri_plugin_opener::OpenerExt;

use crate::ipc::{CommandResult, SimpleAck};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppStatus {
    pub name: String,
    pub abbr: String,
    pub version: String,
    pub logged_in: bool,
    pub username: String,
    pub has_api_key: bool,
    pub masked_api_key: Option<String>,
    pub target_detected: bool,
    pub shop_ready: bool,
}

/// 外链域名白名单（修 H4）：只允许 https + greenpool.cn 及其子域，其余一律拒绝。
fn is_allowed_url(raw: &str) -> bool {
    let Some(rest) = raw.strip_prefix("https://") else {
        return false;
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    // 去 userinfo 与端口
    let host = host.rsplit('@').next().unwrap_or(host);
    let host = host.split(':').next().unwrap_or("");
    !host.is_empty() && (host == "greenpool.cn" || host.ends_with(".greenpool.cn"))
}

/// 聚合状态（单一状态源，§7.2）。
#[tauri::command]
pub async fn app_status() -> CommandResult<AppStatus> {
    let account = crate::services::account::current_status();
    let shop = crate::services::shop::status().await;
    // 修 M7：任一适配器探测到即算已找到（原先只看第一个）
    let target_detected = crate::adapters::all_adapters()
        .iter()
        .any(|a| a.detect().map(|d| d.detected).unwrap_or(false));

    CommandResult::ok(AppStatus {
        name: crate::config::APP_NAME.into(),
        abbr: crate::config::APP_ABBR.into(),
        version: env!("CARGO_PKG_VERSION").into(),
        logged_in: account.logged_in,
        username: account.username,
        has_api_key: account.has_api_key,
        masked_api_key: account.masked_api_key,
        target_detected,
        shop_ready: shop.ready,
    })
}

/// 用系统默认浏览器打开外部链接（仅允许 greenpool.cn 站点，修 H4）。
#[tauri::command]
pub fn app_open_external(app: tauri::AppHandle, url: String) -> CommandResult<SimpleAck> {
    if !is_allowed_url(&url) {
        return CommandResult::err(crate::ipc::CommandError::from(
            crate::error::AppError::Other("仅允许打开绿池官方站点的链接".into()),
        ));
    }
    match app.opener().open_url(url, None::<&str>) {
        Ok(()) => CommandResult::ok(SimpleAck),
        Err(e) => CommandResult::err(crate::ipc::CommandError::from(
            crate::error::AppError::Other(format!("打开链接失败：{}", e)),
        )),
    }
}

/// 事件埋点：只记日志，不上报 IP/原文（§13.3）。
#[tauri::command]
pub fn app_track_event(event: String, props: Option<serde_json::Value>) -> CommandResult<SimpleAck> {
    log::info!("event={} props={:?}", event, props);
    CommandResult::ok(SimpleAck)
}
