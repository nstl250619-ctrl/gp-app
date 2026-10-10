//! 一键配置命令面（§7.2 install.*）：只做参数透传，业务在 services::install。

use tauri::{Manager, State};

use crate::adapters::types::{ApplyResult, DetectResult, InstallPlan};
use crate::ipc::{respond, CommandResult, SimpleAck};
use crate::services::{self, install::InstallRecord};
use crate::state::AppState;

#[tauri::command]
pub fn install_detect(state: State<AppState>) -> CommandResult<Vec<DetectResult>> {
    respond(services::install::detect(&state.data_dir))
}

/// 账号可用模型列表（勾选导入用）；拉取失败也回退默认清单，故不返回错误。
#[tauri::command]
pub async fn install_available_models() -> CommandResult<Vec<String>> {
    CommandResult::ok(services::install::available_models().await)
}

#[tauri::command]
pub fn install_plan(
    state: State<AppState>,
    target: String,
    model_ids: Vec<String>,
    custom_path: Option<String>,
) -> CommandResult<InstallPlan> {
    respond(services::install::plan(target, model_ids, custom_path, &state.data_dir))
}

#[tauri::command]
pub async fn install_apply(
    app: tauri::AppHandle,
    target: String,
    model_ids: Vec<String>,
    verify: bool,
    custom_path: Option<String>,
) -> CommandResult<ApplyResult> {
    // async 命令不能持 State 跨 await，先取 owned 数据。
    let data_dir = app.state::<AppState>().data_dir.clone();
    respond(
        services::install::apply_with_verify(target, model_ids, verify, custom_path, &data_dir).await,
    )
}

#[tauri::command]
pub fn install_rollback(state: State<AppState>, record_id: String) -> CommandResult<SimpleAck> {
    respond(services::install::rollback(record_id, &state.data_dir).map(|_| SimpleAck))
}

#[tauri::command]
pub fn install_history(state: State<AppState>) -> CommandResult<Vec<InstallRecord>> {
    respond(services::install::history(&state.data_dir))
}
