//! 配置适配器共享类型（§7.2 install.* 契约）。

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WriteInput {
    pub base_url: String,
    pub api_key: String,
    pub model_ids: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelSeed {
    pub id: String,
    pub supports_tool_call: bool,
    pub supports_images: bool,
    pub supports_reasoning: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectResult {
    pub target: String,
    pub display_name: String,
    pub path: Option<String>,
    pub detected: bool,
    pub has_managed_entry: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DiffRow {
    pub field: String,
    pub before: String,
    pub after: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallPlan {
    pub target: String,
    pub path: String,
    pub added: Vec<String>,
    pub overwritten: usize,
    pub preserved: usize,
    pub requires_restart: bool,
    pub warnings: Vec<String>,
    pub diff: Vec<DiffRow>,
    /// relatedModels 解析结果（lite=日常小任务 / reasoning=深度思考），供人话 diff 展示。
    pub related_lite: Option<String>,
    pub related_reasoning: Option<String>,
    /// 服务地址对照：before=现有托管条目的服务地址（无托管条目时 None），after=本次将写入的。
    pub base_url_before: Option<String>,
    pub base_url_after: String,
    /// 密钥对照（产品裁决：对照表明文展示——用户自己的配置文件里本就是明文）。
    pub key_before: Option<String>,
    pub key_after: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ApplyResult {
    pub status: String, // applied | connectivity_failed | failed
    /// 实际写入的配置文件完整路径（回滚时按它还原，不重新探测）。
    pub path: String,
    pub backup_path: Option<String>,
    pub verify_ok: bool,
    pub requires_restart: bool,
    pub message: String,
}
