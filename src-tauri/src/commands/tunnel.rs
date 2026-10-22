//! 隧道命令面（GPHub 隧道子系统 → Tauri IPC，v5 契约）。
//!
//!   GET  /api/state        → tunnel_status
//!   POST /api/tunnel/start → tunnel_start
//!   POST /api/tunnel/stop  → tunnel_stop
//!   POST /api/tunnel/restart → tunnel_restart
//!   GET  /api/ssh/probe    → tunnel_probe（指定端点）
//!   （新增）导入私钥         → tunnel_import_key（原生文件框）

use tauri::{AppHandle, Manager};

use crate::ipc::{respond, CommandResult, SimpleAck};
use crate::services::tunnel::{self, ImportKeyResult, ProbeResult, TunnelStatus};

#[tauri::command]
pub fn tunnel_status(app: AppHandle) -> CommandResult<TunnelStatus> {
    let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
    CommandResult::ok(tunnel::status(&data_dir))
}

#[tauri::command]
pub fn tunnel_start(app: AppHandle) -> CommandResult<SimpleAck> {
    let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
    respond(tunnel::start(&app, &data_dir).map(|_| SimpleAck))
}

#[tauri::command]
pub fn tunnel_stop(app: AppHandle) -> CommandResult<SimpleAck> {
    let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
    let r = tunnel::stop(&app);
    if r.is_ok() {
        // 隧道停止：把已安装工具的 base_url 改回直连（方案 B）
        let _ = crate::services::install::rewrite_base_urls(&data_dir, false);
    }
    respond(r.map(|_| SimpleAck))
}

#[tauri::command]
pub fn tunnel_restart(app: AppHandle) -> CommandResult<SimpleAck> {
    let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
    respond(tunnel::restart(&app, &data_dir).map(|_| SimpleAck))
}

#[tauri::command]
pub async fn tunnel_probe(app: AppHandle, endpoint: String) -> CommandResult<ProbeResult> {
    let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
    respond(tunnel::probe(&data_dir, &endpoint).await)
}

#[tauri::command]
pub fn tunnel_import_key(app: AppHandle) -> CommandResult<ImportKeyResult> {
    respond(tunnel::import_key(&app))
}
