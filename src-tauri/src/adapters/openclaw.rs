//! OpenClaw 适配器（多模型列表型）：探测 / 读取 / 计划 / 备份 / 写入 / 回滚。
//!
//! 依据《四客户端 Adapter 配置参考》§三（官方 custom-providers 文档核实）：
//! - 配置入口 `models.providers.gp`：baseUrl + apiKey + api:"openai-completions" + models[]
//! - 模型条目字段与 GP ModelSeed 一一映射：reasoning / input["text","image"]；
//!   supports_tool_call 无显式字段（默认支持）
//! - apiKey 支持直接明文（傻瓜式）或 ${ENV} 插值；此处直写 key

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

use super::types::{ApplyResult, DetectResult, DiffRow, InstallPlan, ModelSeed, WriteInput};

const PROVIDER_ID: &str = "gp";

pub struct OpenClawAdapter {
    candidates: fn() -> Vec<PathBuf>,
}

impl OpenClawAdapter {
    pub fn new() -> Self {
        Self { candidates: config::openclaw_candidate_paths }
    }

    fn existing_path(&self, custom: Option<&Path>) -> Option<PathBuf> {
        match custom {
            Some(p) => Some(p.to_path_buf()),
            None => (self.candidates)().into_iter().find(|p| p.exists()),
        }
    }

    fn parse(raw: &str) -> AppResult<(Value, bool)> {
        let v: Value = serde_json::from_str(raw)
            .map_err(|e| AppError::Other(format!("OpenClaw 配置无法解析：{}", e)))?;
        let has_gp = v
            .get("models")
            .and_then(|m| m.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .is_some();
        Ok((v, has_gp))
    }

    /// 按 ModelSeed 构造 OpenClaw 模型条目（§三 能力字段映射）。
    fn model_entry(seed: &ModelSeed) -> Value {
        let input = if seed.supports_images {
            json!(["text", "image"])
        } else {
            json!(["text"])
        };
        json!({
            "id": seed.id,
            "name": format!("GP {}", seed.id),
            "reasoning": seed.supports_reasoning,
            "input": input,
            "contextWindow": config::MANAGED_MODEL_MAX_INPUT_TOKENS,
            "maxTokens": 8192,
        })
    }

    fn provider_entry(input: &WriteInput, seeds: &[ModelSeed]) -> Value {
        let models: Vec<Value> = seeds.iter().map(Self::model_entry).collect();
        json!({
            "baseUrl": format!("{}/v1", input.base_url),
            "apiKey": input.api_key,
            "api": "openai-completions",
            "models": models,
        })
    }
}

impl Default for OpenClawAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ConfigAdapter for OpenClawAdapter {
    fn id(&self) -> &'static str {
        "openclaw"
    }

    fn display_name(&self) -> &'static str {
        "OpenClaw"
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
            Some(p) if p.exists() => fs_atomic::read_string(p)
                .ok()
                .and_then(|raw| Self::parse(&raw).ok())
                .map(|(_, m)| m)
                .unwrap_or(false),
            _ => false,
        };
        Ok(DetectResult {
            target: "openclaw".into(),
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
        let (current, has_managed) = Self::parse(&raw)?;

        let model_count = seeds.len();
        let base_url_before = current
            .get("models")
            .and_then(|m| m.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .and_then(|g| g.get("baseUrl"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());
        let key_before = current
            .get("models")
            .and_then(|m| m.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .and_then(|g| g.get("apiKey"))
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());
        let before_count = current
            .get("models")
            .and_then(|m| m.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .and_then(|g| g.get("models"))
            .and_then(|m| m.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        let diff = vec![
            DiffRow {
                field: "模型条目".into(),
                before: format!("{} 个", before_count),
                after: format!("{} 个", model_count),
            },
            DiffRow {
                field: "服务地址".into(),
                before: base_url_before.clone().unwrap_or_else(|| "未配置".into()),
                after: format!("{}/v1", input.base_url),
            },
        ];

        Ok(InstallPlan {
            target: "openclaw".into(),
            path: path.display().to_string(),
            added: seeds.iter().map(|s| s.id.clone()).collect(),
            overwritten: if has_managed { 1 } else { 0 },
            preserved: 0,
            requires_restart: false,
            warnings: vec!["OpenClaw 为多模型列表：全部勾选的模型都会写入，客户端内可手动切换".into()],
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
        let (mut v, has_managed) = Self::parse(&raw)?;

        // 已是 GP 托管态时不新建备份：还原点必须是「GP 介入前」的原始配置（链式备份防护）。
        let backup = if !has_managed { Some(fs_atomic::backup(&path)?) } else { None };

        // 键级写入：只覆盖 models.providers.gp，其余全部保留
        let obj = v
            .as_object_mut()
            .ok_or_else(|| AppError::Other("OpenClaw 配置顶层必须是对象".into()))?;
        let models = obj
            .entry("models")
            .or_insert_with(|| json!({}));
        if !models.is_object() {
            return Err(AppError::Other("OpenClaw 配置的 models 必须是对象".into()));
        }
        models
            .as_object_mut()
            .expect("checked above")
            .entry("providers")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .ok_or_else(|| AppError::Other("OpenClaw 配置的 models.providers 必须是对象".into()))?
            .insert(PROVIDER_ID.into(), Self::provider_entry(input, seeds));

        let content = serde_json::to_string_pretty(&v)?;
        fs_atomic::atomic_write(&path, &content)?;

        // 回读校验：解析通过 + gp 段在位 + baseUrl 正确
        let back = fs_atomic::read_string(&path)?;
        let (parsed, _) = Self::parse(&back)?;
        let verify_ok = parsed
            .get("models")
            .and_then(|m| m.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .and_then(|g| g.get("baseUrl"))
            .and_then(|u| u.as_str())
            .map(|u| u == format!("{}/v1", input.base_url))
            .unwrap_or(false);

        Ok(ApplyResult {
            extra_backups: Vec::new(),
            status: if verify_ok { "applied".into() } else { "failed".into() },
            path: path.display().to_string(),
            backup_path: backup.map(|b| b.display().to_string()),
            verify_ok,
            requires_restart: false,
            message: if verify_ok { "已写入".into() } else { "写入校验未通过".into() },
        })
    }

    /// 按记录的目标路径还原（不重新探测），备份内容原子写回 + 摘要校验。
    fn is_managed_content(&self, raw: &str) -> bool {
        Self::parse(raw).map(|(_, m)| m).unwrap_or(false)
    }

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

    #[test]
    fn provider_entry_maps_model_seed() {
        let input = WriteInput {
            base_url: "https://aigw.greenpool.cn".into(),
            api_key: "sk-gp".into(),
            model_ids: vec![],
        };
        let seeds = vec![
            ModelSeed { id: "deepseek-v4-pro".into(), supports_tool_call: true, supports_images: true, supports_reasoning: true },
            ModelSeed { id: "text-only".into(), supports_tool_call: true, supports_images: false, supports_reasoning: false },
        ];
        let gp = OpenClawAdapter::provider_entry(&input, &seeds);
        assert_eq!(gp["baseUrl"], "https://aigw.greenpool.cn/v1");
        assert_eq!(gp["api"], "openai-completions");
        let models = gp["models"].as_array().unwrap();
        assert_eq!(models.len(), 2);
        assert_eq!(models[0]["reasoning"], true);
        assert_eq!(models[0]["input"][1], "image");
        assert_eq!(models[1]["input"][0], "text");
        assert!(models[1]["input"].as_array().unwrap().len() == 1);
        assert_eq!(models[0]["name"], "GP deepseek-v4-pro");
    }

    #[test]
    fn merge_preserves_other_providers() {
        let raw = r#"{
            "models": {
                "mode": "merge",
                "providers": { "other": { "baseUrl": "https://x.example", "apiKey": "k", "api": "openai-completions", "models": [] } }
            },
            "misc": 1
        }"#;
        let (mut v, has_managed) = OpenClawAdapter::parse(raw).unwrap();
        assert!(!has_managed);
        let obj = v.as_object_mut().unwrap();
        let models = obj.entry("models").or_insert_with(|| json!({}));
        models
            .as_object_mut()
            .unwrap()
            .entry("providers")
            .or_insert_with(|| json!({}))
            .as_object_mut()
            .unwrap()
            .insert("gp".into(), json!({ "baseUrl": "https://aigw.greenpool.cn/v1" }));
        let out = serde_json::to_string(&v).unwrap();
        assert!(out.contains("https://x.example")); // 其他 provider 保留
        assert!(out.contains("\"misc\":1")); // 其他顶层键保留
        let (parsed, managed2) = OpenClawAdapter::parse(&out).unwrap();
        assert!(managed2);
        assert_eq!(parsed["models"]["providers"]["gp"]["baseUrl"], "https://aigw.greenpool.cn/v1");
    }
}
