//! 配置适配器注册表（ADR-002：六能力接口 detect/read_current/plan/backup/apply/rollback）。
//! 七个客户端全部可写：WorkBuddy / CodeBuddy / DSH / Codex / Claude Code / OpenClaw / Hermes。

pub mod claude;
pub mod codex;
pub mod dsh;
pub mod hermes;
pub mod json_merge;
pub mod models_template;
pub mod openclaw;
pub mod types;
pub mod workbuddy;

use std::path::Path;

use crate::error::AppResult;
use types::{ApplyResult, DetectResult, InstallPlan, ModelSeed, WriteInput};

pub trait ConfigAdapter: Send + Sync {
    fn id(&self) -> &'static str;
    fn display_name(&self) -> &'static str;
    /// custom_path：用户手动指定的配置文件路径（分身/重命名场景），优先于自动探测。
    fn detect(&self, custom_path: Option<&Path>) -> AppResult<DetectResult>;
    fn plan(&self, input: &WriteInput, seeds: &[ModelSeed], custom_path: Option<&Path>) -> AppResult<InstallPlan>;
    fn apply(&self, input: &WriteInput, seeds: &[ModelSeed], custom_path: Option<&Path>) -> AppResult<ApplyResult>;
    /// 按备份还原到指定目标路径（不重新探测，避免多版本/移动后写错文件）。
    fn rollback(&self, backup: &Path, target: &Path) -> AppResult<()>;
    /// 判断「某份配置内容」是否已处于 GP 托管态（与 detect 同口径）。
    /// 用途：还原点校验——备份内容若已是托管态，说明它不是「配置前原始快照」，不能作为还原点。
    /// 默认 false（宁可放行，也不能误杀用户合法备份）。
    fn is_managed_content(&self, _raw: &str) -> bool {
        false
    }
}

pub fn all_adapters() -> Vec<Box<dyn ConfigAdapter>> {
    vec![
        Box::new(workbuddy::ModelsJsonAdapter::workbuddy()),
        Box::new(workbuddy::ModelsJsonAdapter::codebuddy()),
        Box::new(dsh::DshAdapter::new()),
        Box::new(codex::CodexAdapter::new()),
        Box::new(claude::ClaudeAdapter::new()),
        Box::new(openclaw::OpenClawAdapter::new()),
        Box::new(hermes::HermesAdapter::new()),
    ]
}

pub fn find_adapter(target: &str) -> Option<Box<dyn ConfigAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == target)
}
