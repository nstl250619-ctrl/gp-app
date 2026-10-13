//! Codex CLI 适配器（单模型 agent）：探测 / 读取 / 计划 / 备份 / 写入 / 回滚。
//!
//! 实测依据（2026-10-13 本机 Codex v0.155.0-alpha.9.2）：
//! - `wire_api = "chat"` 已废弃，强制 `wire_api = "responses"`（OpenAI Responses API）；
//! - new-api 中转站原生支持标准 Responses API（/v1/responses 实测返回 output_text），零协议转换；
//! - 认证走 `experimental_bearer_token` 直写 key（无需 auth.json / env_key）；
//! - 单模型 agent：只写一个默认主力模型（model_ids[0]），多模型列表无意义；
//! - 模型元数据（context window 等）无本地注入途径（元数据仅来自官方 models_cache.json），
//!   写入后 Codex 对自定义模型会提示 fallback warning，不影响基本使用（已实测）。
//!
//! 写入采用 toml_edit（保注释/保格式），只覆盖托管字段，其余内容逐字保留。

use std::path::{Path, PathBuf};

use toml_edit::{DocumentMut, Item, Table, value};

use crate::config;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

use super::types::{ApplyResult, DetectResult, DiffRow, InstallPlan, ModelSeed, WriteInput};

const PROVIDER_ID: &str = "gp";

pub struct CodexAdapter {
    candidates: fn() -> Vec<PathBuf>,
}

impl CodexAdapter {
    pub fn new() -> Self {
        Self { candidates: config::codex_candidate_paths }
    }

    fn existing_path(&self, custom: Option<&Path>) -> Option<PathBuf> {
        match custom {
            Some(p) => Some(p.to_path_buf()),
            None => (self.candidates)().into_iter().find(|p| p.exists()),
        }
    }

    /// 解析 + 托管判定（model_provider == "gp" 或存在 [model_providers.gp] 段）。
    fn parse(raw: &str) -> AppResult<(DocumentMut, bool)> {
        let doc = raw
            .parse::<DocumentMut>()
            .map_err(|e| AppError::Other(format!("Codex config.toml 无法解析：{}", e)))?;
        let provider_is_gp = doc.get("model_provider").and_then(|i| i.as_str()) == Some(PROVIDER_ID);
        let has_gp_table = doc
            .get("model_providers")
            .and_then(|m| m.get(PROVIDER_ID))
            .is_some();
        Ok((doc, provider_is_gp || has_gp_table))
    }

    /// 读取现有 gp 托管段字段（plan 对照数据源）。
    fn managed_field<'a>(doc: &'a DocumentMut, key: &str) -> Option<&'a str> {
        doc.get("model_providers")
            .and_then(|m| m.get(PROVIDER_ID))
            .and_then(|g| g.get(key))
            .and_then(|i| i.as_str())
    }
}

impl Default for CodexAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ConfigAdapter for CodexAdapter {
    fn id(&self) -> &'static str {
        "codex"
    }

    fn display_name(&self) -> &'static str {
        "Codex"
    }

    fn detect(&self, custom_path: Option<&Path>) -> AppResult<DetectResult> {
        let (path, detected) = match custom_path {
            Some(p) => (Some(p.to_path_buf()), p.exists()),
            None => {
                let p = (self.candidates)().into_iter().find(|p| p.exists());
                (p.clone(), p.is_some())
            }
        };
        let has_managed = match &path {
            Some(p) if p.exists() => {
                let raw = fs_atomic::read_string(p).unwrap_or_default();
                Self::parse(&raw).map(|(_, m)| m).unwrap_or(false)
            }
            _ => false,
        };
        Ok(DetectResult {
            target: "codex".into(),
            display_name: self.display_name().into(),
            path: path.as_ref().map(|p| p.display().to_string()),
            detected,
            has_managed_entry: has_managed,
        })
    }

    fn plan(
        &self,
        input: &WriteInput,
        seeds: &[ModelSeed],
        custom_path: Option<&Path>,
    ) -> AppResult<InstallPlan> {
        let path = self.existing_path(custom_path).ok_or(AppError::TargetNotFound)?;
        let raw = fs_atomic::read_string(&path)?;
        let (doc, has_managed) = Self::parse(&raw)?;

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;
        let model_before = doc.get("model").and_then(|i| i.as_str()).unwrap_or("").to_string();
        let base_url_before = Self::managed_field(&doc, "base_url").map(|s| s.to_string());
        let key_before = Self::managed_field(&doc, "experimental_bearer_token").map(|s| s.to_string());

        let diff = vec![
            DiffRow {
                field: "默认主力模型".into(),
                before: if model_before.is_empty() { "未设置".into() } else { model_before },
                after: model_id.clone(),
            },
            DiffRow {
                field: "服务地址".into(),
                before: base_url_before.clone().unwrap_or_else(|| "未配置".into()),
                after: format!("{}/v1", input.base_url),
            },
        ];

        Ok(InstallPlan {
            target: "codex".into(),
            path: path.display().to_string(),
            added: vec![model_id],
            overwritten: if has_managed { 1 } else { 0 },
            preserved: 0,
            requires_restart: false,
            warnings: vec![
                "Codex 为单模型 agent：只写入一个默认主力模型，可在 Codex 内用 /model 手动切换".into(),
                "Codex 对第三方模型会提示 metadata fallback 警告，属正常现象，不影响使用".into(),
                "将同时关闭 responses_websockets 特性（中转站仅支持 HTTP，实测关闭后更稳定）".into(),
            ],
            diff,
            related_lite: None,
            related_reasoning: None,
            base_url_before,
            base_url_after: format!("{}/v1", input.base_url),
            key_before,
            key_after: input.api_key.clone(),
        })
    }

    fn apply(
        &self,
        input: &WriteInput,
        seeds: &[ModelSeed],
        custom_path: Option<&Path>,
    ) -> AppResult<ApplyResult> {
        let path = self.existing_path(custom_path).ok_or(AppError::TargetNotFound)?;
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(AppError::TargetNotFound); // 禁止静默创建目录（§11.1）
            }
        }

        let raw = fs_atomic::read_string(&path)?;
        let (mut doc, has_managed) = Self::parse(&raw)?;

        // 已是 GP 托管态时不新建备份：还原点必须是「GP 介入前」的原始配置（链式备份防护）。
        let backup = if !has_managed { Some(fs_atomic::backup(&path)?) } else { None };

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;

        // 托管字段：只覆盖 model / model_provider / [model_providers.gp] / features 开关，其余逐字保留
        doc["model"] = value(model_id.as_str());
        doc["model_provider"] = value(PROVIDER_ID);

        let mut gp = Table::new();
        gp["name"] = value("GP GreenPool");
        gp["wire_api"] = value("responses"); // 0.155 强制 responses（chat 已废弃，实测）
        gp["requires_openai_auth"] = value(false);
        gp["base_url"] = value(format!("{}/v1", input.base_url));
        gp["experimental_bearer_token"] = value(input.api_key.as_str());
        doc["model_providers"][PROVIDER_ID] = Item::Table(gp);

        // Codex 0.155 默认可能尝试 wss 连 /v1/responses（官方 WebSocket 通道），
        // new-api 中转站仅支持 HTTP；实测禁用后稳定走 HTTP（E2E 验证）。
        // 索引赋值对已有 [features] 表是按键合并，用户其他 feature 开关保留。
        doc["features"]["responses_websockets"] = value(false);
        doc["features"]["responses_websockets_v2"] = value(false);

        let content = doc.to_string();
        fs_atomic::atomic_write(&path, &content)?;

        // 回读校验：解析通过 + 托管字段在位
        let back = fs_atomic::read_string(&path)?;
        let (doc2, _) = Self::parse(&back)?;
        let ws_off = doc2
            .get("features")
            .and_then(|f| f.get("responses_websockets"))
            .and_then(|i| i.as_bool())
            == Some(false);
        let verify_ok = doc2.get("model_provider").and_then(|i| i.as_str()) == Some(PROVIDER_ID)
            && doc2.get("model").and_then(|i| i.as_str()) == Some(model_id.as_str())
            && Self::managed_field(&doc2, "base_url").is_some()
            && ws_off;

        Ok(ApplyResult {
            status: if verify_ok { "applied".into() } else { "failed".into() },
            path: path.display().to_string(),
            backup_path: backup.map(|b| b.display().to_string()),
            verify_ok,
            requires_restart: false,
            message: if verify_ok { "已写入".into() } else { "写入校验未通过".into() },
        })
    }

    /// 按记录的目标路径还原（不重新探测），备份内容原子写回 + 摘要校验。
    fn rollback(&self, backup: &Path, target: &Path) -> AppResult<()> {
        if !backup.exists() {
            return Err(AppError::RollbackFailed("备份文件不存在".into()));
        }
        let content = fs_atomic::read_string(backup)?;
        let expected = fs_atomic::digest(backup)?;
        fs_atomic::atomic_write(target, &content)?;
        if fs_atomic::verify_digest(target, &expected) {
            Ok(())
        } else {
            Err(AppError::RollbackFailed("还原后校验不一致".into()))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = "model_provider = \"custom\"\nmodel = \"gpt-x\"\n# 注释保留\n[model_providers.custom]\nname = \"custom\"\nwire_api = \"responses\"\n";

    fn input() -> WriteInput {
        WriteInput {
            base_url: "https://aigw.greenpool.cn".into(),
            api_key: "sk-gp-test".into(),
            model_ids: vec!["deepseek-v4-pro".into()],
        }
    }

    fn seed(id: &str) -> ModelSeed {
        ModelSeed { id: id.into(), supports_tool_call: true, supports_images: true, supports_reasoning: true }
    }

    #[test]
    fn existing_features_table_preserved_on_merge() {
        // 用户真实 config.toml 带 [features] 段（如 js_repl/memories），合并后必须保留
        let raw = "model_provider = \"custom\"\n[features]\njs_repl = false\nmemories = true\n";
        let (mut doc, _) = CodexAdapter::parse(raw).unwrap();
        doc["features"]["responses_websockets"] = value(false);
        let out = doc.to_string();
        let (parsed, _) = CodexAdapter::parse(&out).unwrap();
        let feat = parsed.get("features").unwrap();
        assert_eq!(feat.get("js_repl").and_then(|i| i.as_bool()), Some(false));
        assert_eq!(feat.get("memories").and_then(|i| i.as_bool()), Some(true));
        assert_eq!(feat.get("responses_websockets").and_then(|i| i.as_bool()), Some(false));
    }

    #[test]
    fn detect_managed_on_gp_provider() {
        let (doc, managed) = CodexAdapter::parse(SAMPLE).unwrap();
        assert!(!managed);
        let doc2 = format!("{SAMPLE}[model_providers.gp]\nname=\"x\"\n");
        let (_, managed2) = CodexAdapter::parse(&doc2).unwrap();
        let _ = doc;
        assert!(managed2);
    }

    #[test]
    fn apply_overwrites_managed_fields_and_preserves_rest() {
        let (mut doc, _) = CodexAdapter::parse(SAMPLE).unwrap();
        doc["model"] = value("deepseek-v4-pro");
        doc["model_provider"] = value(PROVIDER_ID);
        let mut gp = Table::new();
        gp["name"] = value("GP GreenPool");
        gp["wire_api"] = value("responses");
        gp["requires_openai_auth"] = value(false);
        gp["base_url"] = value("https://aigw.greenpool.cn/v1");
        gp["experimental_bearer_token"] = value("sk-gp-test");
        doc["model_providers"][PROVIDER_ID] = Item::Table(gp);
        doc["features"]["responses_websockets"] = value(false);
        doc["features"]["responses_websockets_v2"] = value(false);
        let out = doc.to_string();

        let (parsed, managed) = CodexAdapter::parse(&out).unwrap();
        assert!(managed);
        assert_eq!(parsed.get("model").and_then(|i| i.as_str()), Some("deepseek-v4-pro"));
        // 旧 provider 段保留
        assert!(parsed.get("model_providers").and_then(|m| m.get("custom")).is_some());
        // 注释保留
        assert!(out.contains("# 注释保留"));
        // 托管段字段
        let gp_out = parsed.get("model_providers").unwrap().get(PROVIDER_ID).unwrap();
        assert_eq!(gp_out.get("wire_api").and_then(|i| i.as_str()), Some("responses"));
        assert_eq!(gp_out.get("base_url").and_then(|i| i.as_str()), Some("https://aigw.greenpool.cn/v1"));
        // websocket 开关关闭
        let feat = parsed.get("features").unwrap();
        assert_eq!(feat.get("responses_websockets").and_then(|i| i.as_bool()), Some(false));
        assert_eq!(feat.get("responses_websockets_v2").and_then(|i| i.as_bool()), Some(false));
    }
}
