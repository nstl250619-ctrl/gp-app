//! 配置适配器注册表（ADR-002：六能力接口 detect/read_current/plan/backup/apply/rollback）。
//! WorkBuddy + CodeBuddy 可写；OpenClaw/Hermes 留到 P1 仅探测（§11.7）。

pub mod json_merge;
pub mod models_template;
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
}

pub fn all_adapters() -> Vec<Box<dyn ConfigAdapter>> {
    vec![
        Box::new(workbuddy::ModelsJsonAdapter::workbuddy()),
        Box::new(workbuddy::ModelsJsonAdapter::codebuddy()),
    ]
}

pub fn find_adapter(target: &str) -> Option<Box<dyn ConfigAdapter>> {
    all_adapters().into_iter().find(|a| a.id() == target)
}
