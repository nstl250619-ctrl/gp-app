//! 一键配置命令面（§7.2 install.*）：只做参数透传，业务在 services::install。

use serde::Serialize;
use tauri::{Manager, State};

use crate::adapters::types::{ApplyResult, DetectResult, InstallPlan};
use crate::ipc::{respond, CommandResult, SimpleAck};
use crate::services::{self, install::InstallRecord};
use crate::state::AppState;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AvailableModels {
    pub models: Vec<String>,
    /// true = 网关拉取失败，当前是离线兜底清单（可能过期，UI 应提示）
    pub offline: bool,
}

#[tauri::command]
pub fn install_detect(state: State<AppState>) -> CommandResult<Vec<DetectResult>> {
    respond(services::install::detect(&state.data_dir))
}

/// 账号可用模型列表（勾选导入用）；拉取失败回退默认清单并标记 offline。
#[tauri::command]
pub async fn install_available_models() -> CommandResult<AvailableModels> {
    let (models, offline) = services::install::available_models().await;
    CommandResult::ok(AvailableModels { models, offline })
}

#[tauri::command]
pub fn install_plan(
    state: State<AppState>,
    target: String,
    model_ids: Vec<String>,
    custom_path: Option<String>,
    base_url: Option<String>,
) -> CommandResult<InstallPlan> {
    respond(services::install::plan(target, model_ids, custom_path, base_url, &state.data_dir))
}

#[tauri::command]
pub async fn install_apply(
    app: tauri::AppHandle,
    target: String,
    model_ids: Vec<String>,
    verify: bool,
    custom_path: Option<String>,
    base_url: Option<String>,
) -> CommandResult<ApplyResult> {
    // async 命令不能持 State 跨 await，先取 owned 数据。
    let data_dir = app.state::<AppState>().data_dir.clone();
    respond(
        services::install::apply_with_verify(target, model_ids, verify, custom_path, base_url, &data_dir).await,
    )
}

/// force=true：用户已二次确认（还原点是 GP 快照也强制回退）。
#[tauri::command]
pub fn install_rollback(
    state: State<AppState>,
    record_id: String,
    force: Option<bool>,
) -> CommandResult<SimpleAck> {
    respond(
        services::install::rollback(record_id, force.unwrap_or(false), &state.data_dir)
            .map(|_| SimpleAck),
    )
}

#[tauri::command]
pub fn install_history(state: State<AppState>) -> CommandResult<Vec<InstallRecord>> {
    respond(services::install::history(&state.data_dir))
}
