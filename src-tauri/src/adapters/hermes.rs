//! Hermes 适配器（疑似自动拉模型型）：探测 / 读取 / 计划 / 备份 / 写入 / 回滚。
//!
//! 依据《四客户端 Adapter 配置参考》§四：
//! - 主配置 ~/.hermes/config.yaml（YAML）：model.provider="custom" + model.base_url
//! - 密钥 ~/.hermes/.env：行级合并写入（不覆盖用户其他变量）
//! - 模型列表疑似自动拉 /v1/models（⚠️ 待装机实测）：GP 倾向只写 provider/base_url/key，
//!   另写 model.default = model_ids[0] 保证开箱可用
//! - 写入用 serde_yaml 全量序列化：会重排键序并丢失注释（首版已知限制，plan warning 说明）

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

use super::types::{ApplyResult, DetectResult, DiffRow, InstallPlan, ModelSeed, WriteInput};

const ENV_KEY_BRAND: &str = "GP_API_KEY";
const ENV_KEY_STD: &str = "OPENAI_API_KEY";

pub struct HermesAdapter {
    candidates: fn() -> Vec<PathBuf>,
}

impl HermesAdapter {
    pub fn new() -> Self {
        Self { candidates: config::hermes_candidate_paths }
    }

    fn existing_path(&self, custom: Option<&Path>) -> Option<PathBuf> {
        match custom {
            Some(p) => Some(p.to_path_buf()),
            None => (self.candidates)().into_iter().find(|p| p.exists()),
        }
    }

    /// .env 与 config.yaml 同目录。
    fn env_path(config_path: &Path) -> PathBuf {
        config_path.parent().unwrap_or(Path::new(".")).join(".env")
    }

    /// .env 行级合并：移除旧 key 行 → 追加新行，其他行原样保留。
    fn merge_env(raw: &str, pairs: &[(&str, &str)]) -> String {
        let keys: Vec<&str> = pairs.iter().map(|(k, _)| *k).collect();
        let mut lines: Vec<String> = raw
            .lines()
            .filter(|l| {
                let t = l.trim();
                !keys.iter().any(|k| t.starts_with(k) && t[k.len()..].starts_with('='))
            })
            .map(|s| s.to_string())
            .collect();
        for (k, v) in pairs {
            lines.push(format!("{k}={v}"));
        }
        let mut out = lines.join("\n");
        if !out.is_empty() {
            out.push('\n');
        }
        out
    }
}

impl Default for HermesAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ConfigAdapter for HermesAdapter {
    fn id(&self) -> &'static str {
        "hermes"
    }

    fn display_name(&self) -> &'static str {
        "Hermes"
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
                .and_then(|raw| serde_yaml::from_str::<Value>(&raw).ok())
                .and_then(|v| {
                    v.get("model")
                        .and_then(|m| m.get("base_url"))
                        .and_then(|u| u.as_str())
                        .map(|u| u.contains("greenpool"))
                })
                .unwrap_or(false),
            _ => false,
        };
        Ok(DetectResult {
            target: "hermes".into(),
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
        let current: Value = serde_yaml::from_str(&raw)
            .map_err(|e| AppError::Other(format!("config.yaml 无法解析：{}", e)))?;

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;
        let default_before = current
            .get("model")
            .and_then(|m| m.get("default"))
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();
        let base_url_before = current
            .get("model")
            .and_then(|m| m.get("base_url"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());

        let diff = vec![
            DiffRow {
                field: "默认模型".into(),
                before: if default_before.is_empty() { "未设置".into() } else { default_before },
                after: model_id.clone(),
            },
            DiffRow {
                field: "服务地址".into(),
                before: base_url_before.clone().unwrap_or_else(|| "未配置".into()),
                after: format!("{}/v1", input.base_url),
            },
        ];

        Ok(InstallPlan {
            target: "hermes".into(),
            path: path.display().to_string(),
            added: vec![model_id],
            overwritten: 0,
            preserved: 0,
            requires_restart: false,
            warnings: vec![
                "Hermes 疑似自动拉取 /v1/models（待实测）：全量模型可在客户端内用 /models 查看/切换".into(),
                "YAML 写入会重排键序并丢失注释（首版已知限制）".into(),
            ],
            diff,
            related_lite: None,
            related_reasoning: None,
            base_url_before,
            base_url_after: format!("{}/v1", input.base_url),
            key_before: None, // .env 明文不进对照表（避免展示用户无关变量）
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
        let mut v: Value = serde_yaml::from_str(&raw)
            .map_err(|e| AppError::Other(format!("config.yaml 无法解析：{}", e)))?;

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;
        let base = format!("{}/v1", input.base_url);

        // 托管判定：model.base_url 已指向 greenpool → 不新建备份（链式备份防护）。
        let has_managed = v
            .get("model")
            .and_then(|m| m.get("base_url"))
            .and_then(|u| u.as_str())
            .map(|u| u.contains("greenpool"))
            .unwrap_or(false);
        let backup = if !has_managed { Some(fs_atomic::backup(&path)?) } else { None };

        // 写 model 段（保留其他顶层键）
        let obj = v
            .as_object_mut()
            .ok_or_else(|| AppError::Other("config.yaml 顶层必须是映射".into()))?;
        let model = obj
            .entry("model")
            .or_insert_with(|| json!({}));
        if !model.is_object() {
            return Err(AppError::Other("config.yaml 的 model 必须是映射".into()));
        }
        let m = model.as_object_mut().expect("checked above");
        m.insert("provider".into(), json!("custom"));
        m.insert("base_url".into(), json!(base));
        m.insert("default".into(), json!(model_id));

        let content = serde_yaml::to_string(&v)
            .map_err(|e| AppError::Other(format!("config.yaml 序列化失败：{}", e)))?;
        fs_atomic::atomic_write(&path, &content)?;

        // .env 行级合并（若同目录 .env 不存在则创建）
        let env_path = Self::env_path(&path);
        let env_raw = if env_path.exists() { fs_atomic::read_string(&env_path)? } else { String::new() };
        let env_new = Self::merge_env(&env_raw, &[(ENV_KEY_BRAND, &input.api_key), (ENV_KEY_STD, &input.api_key)]);
        if env_new != env_raw {
            let env_backup = if env_path.exists() && !env_raw.is_empty() { Some(fs_atomic::backup(&env_path)?) } else { None };
            fs_atomic::atomic_write(&env_path, &env_new)?;
            let _ = env_backup; // .env 备份随 config 备份一起保留于记录（首个为主备份）
        }

        // 回读校验
        let back = fs_atomic::read_string(&path)?;
        let parsed: Value = serde_yaml::from_str(&back)
            .map_err(|e| AppError::Other(format!("写入后 config.yaml 无法解析：{}", e)))?;
        let verify_ok = parsed
            .get("model")
            .and_then(|m| m.get("base_url"))
            .and_then(|u| u.as_str())
            .map(|u| u == base)
            .unwrap_or(false)
            && parsed
                .get("model")
                .and_then(|m| m.get("default"))
                .and_then(|d| d.as_str())
                .map(|d| d == model_id)
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
        serde_yaml::from_str::<Value>(raw)
            .map(|v| {
                v.get("model")
                    .and_then(|m| m.get("base_url"))
                    .and_then(|u| u.as_str())
                    .map(|u| u.contains("greenpool"))
                    .unwrap_or(false)
            })
            .unwrap_or(false)
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
    fn env_merge_replaces_only_target_keys() {
        let raw = "FOO=1\nGP_API_KEY=old\n# comment\nOPENAI_API_KEY=old2\nBAR=2\n";
        let out = HermesAdapter::merge_env(raw, &[("GP_API_KEY", "new"), ("OPENAI_API_KEY", "new2")]);
        assert!(out.contains("FOO=1"));
        assert!(out.contains("BAR=2"));
        assert!(out.contains("# comment"));
        assert!(!out.contains("old"));
        assert!(out.contains("GP_API_KEY=new"));
        assert!(out.contains("OPENAI_API_KEY=new2"));
        // 每个键只出现一次
        assert_eq!(out.matches("GP_API_KEY=").count(), 1);
    }

    #[test]
    fn model_section_merge_preserves_others() {
        let raw = "agent:\n  reasoning_effort: high\nmodel:\n  provider: openai\n  context_length: 128000\n";
        let mut v: Value = serde_yaml::from_str(raw).unwrap();
        let obj = v.as_object_mut().unwrap();
        let model = obj.entry("model").or_insert_with(|| json!({}));
        let m = model.as_object_mut().unwrap();
        m.insert("provider".into(), json!("custom"));
        m.insert("base_url".into(), json!("https://aigw.greenpool.cn/v1"));
        m.insert("default".into(), json!("deepseek-v4-pro"));
        let out = serde_yaml::to_string(&v).unwrap();
        let back: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(back["model"]["provider"], "custom");
        assert_eq!(back["model"]["base_url"], "https://aigw.greenpool.cn/v1");
        assert_eq!(back["model"]["default"], "deepseek-v4-pro");
        assert_eq!(back["agent"]["reasoning_effort"], "high"); // 其他顶层键保留
        assert_eq!(back["model"]["context_length"], 128000); // model 段其他键保留
    }
}
