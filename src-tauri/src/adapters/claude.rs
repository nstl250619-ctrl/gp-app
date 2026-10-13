//! Claude Code 适配器（单模型 agent）：探测 / 读取 / 计划 / 备份 / 写入 / 回滚。
//!
//! 实测与源码依据（2026-10-13）：
//! - Claude Code 只认 Anthropic Messages（/v1/messages）；GP 后端 new-api 中转站已内置
//!   Claude↔OpenAI 协议转换（relay-router.go:97 POST /v1/messages → RelayFormatClaude），
//!   因此 base_url 直接指向中转站根地址即可，GP 端零协议开发；
//! - `ANTHROPIC_BASE_URL` 写根地址（Claude Code 自行拼接 /v1/messages）；
//! - 鉴权用 `ANTHROPIC_API_KEY`（Claude Code 以 x-api-key + anthropic-version 头发送，
//!   new-api /v1/models 已识别该头组合）；
//! - 单模型 agent：只写一个默认主力模型（model_ids[0]）。
//!
//! 写入为 JSON 深合并：只覆盖 model 与 env 的两个托管键，其余键原样保留。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

use super::types::{ApplyResult, DetectResult, DiffRow, InstallPlan, ModelSeed, WriteInput};

const ENV_BASE_URL: &str = "ANTHROPIC_BASE_URL";
const ENV_API_KEY: &str = "ANTHROPIC_API_KEY";

pub struct ClaudeAdapter {
    candidates: fn() -> Vec<PathBuf>,
}

impl ClaudeAdapter {
    pub fn new() -> Self {
        Self { candidates: config::claude_candidate_paths }
    }

    fn existing_path(&self, custom: Option<&Path>) -> Option<PathBuf> {
        match custom {
            Some(p) => Some(p.to_path_buf()),
            None => (self.candidates)().into_iter().find(|p| p.exists()),
        }
    }

    /// 托管判定：env.ANTHROPIC_BASE_URL 指向 greenpool。
    fn is_managed(v: &Value) -> bool {
        v.get("env")
            .and_then(|e| e.get(ENV_BASE_URL))
            .and_then(|u| u.as_str())
            .map(|u| u.contains("greenpool"))
            .unwrap_or(false)
    }

    fn env_field<'a>(v: &'a Value, key: &str) -> Option<&'a str> {
        v.get("env").and_then(|e| e.get(key)).and_then(|x| x.as_str())
    }
}

impl Default for ClaudeAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ConfigAdapter for ClaudeAdapter {
    fn id(&self) -> &'static str {
        "claude"
    }

    fn display_name(&self) -> &'static str {
        "Claude Code"
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
                .and_then(|raw| serde_json::from_str::<Value>(&raw).ok())
                .map(|v| Self::is_managed(&v))
                .unwrap_or(false),
            _ => false,
        };
        Ok(DetectResult {
            target: "claude".into(),
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
        let current: Value =
            serde_json::from_str(&raw).map_err(|e| AppError::Other(format!("settings.json 无法解析：{}", e)))?;

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;
        let model_before = current.get("model").and_then(|m| m.as_str()).unwrap_or("").to_string();
        let base_url_before = Self::env_field(&current, ENV_BASE_URL).map(|s| s.to_string());
        let key_before = Self::env_field(&current, ENV_API_KEY).map(|s| s.to_string());
        let preserved = current.as_object().map(|o| o.len()).unwrap_or(0);

        let diff = vec![
            DiffRow {
                field: "默认主力模型".into(),
                before: if model_before.is_empty() { "未设置".into() } else { model_before },
                after: model_id.clone(),
            },
            DiffRow {
                field: "服务地址".into(),
                before: base_url_before.clone().unwrap_or_else(|| "未配置".into()),
                after: input.base_url.clone(),
            },
        ];

        Ok(InstallPlan {
            target: "claude".into(),
            path: path.display().to_string(),
            added: vec![model_id],
            overwritten: if Self::is_managed(&current) { 1 } else { 0 },
            preserved,
            requires_restart: false,
            warnings: vec![
                "Claude Code 为单模型 agent：只写入一个默认主力模型，可在 Claude Code 内用 /model 手动切换".into(),
                "协议转换由 GP 中转站内置（/v1/messages），无需额外配置".into(),
            ],
            diff,
            related_lite: None,
            related_reasoning: None,
            base_url_before,
            base_url_after: input.base_url.clone(),
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
        let mut v: Value =
            serde_json::from_str(&raw).map_err(|e| AppError::Other(format!("settings.json 无法解析：{}", e)))?;

        // 已是 GP 托管态时不新建备份：还原点必须是「GP 介入前」的原始配置（链式备份防护）。
        let backup = if !Self::is_managed(&v) { Some(fs_atomic::backup(&path)?) } else { None };

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;

        let obj = v
            .as_object_mut()
            .ok_or_else(|| AppError::Other("settings.json 顶层必须是对象".into()))?;
        obj.insert("model".into(), Value::String(model_id.clone()));

        // env 深合并：只覆盖两个托管键，用户其他环境变量原样保留
        let env = obj
            .entry("env")
            .or_insert_with(|| Value::Object(Default::default()));
        if !env.is_object() {
            return Err(AppError::Other("settings.json 的 env 必须是对象".into()));
        }
        let env_obj = env.as_object_mut().expect("checked above");
        env_obj.insert(ENV_BASE_URL.into(), Value::String(input.base_url.clone()));
        env_obj.insert(ENV_API_KEY.into(), Value::String(input.api_key.clone()));

        let content = serde_json::to_string_pretty(&v)?;
        fs_atomic::atomic_write(&path, &content)?;

        // 回读校验：解析通过 + 托管键在位 + env 长度不减
        let back = fs_atomic::read_string(&path)?;
        let parsed: Value = serde_json::from_str(&back)?;
        let verify_ok = parsed.get("model").and_then(|m| m.as_str()) == Some(model_id.as_str())
            && Self::env_field(&parsed, ENV_BASE_URL).is_some()
            && Self::env_field(&parsed, ENV_API_KEY).is_some()
            && parsed.get("env").map(|e| e.as_object().map(|o| o.len()).unwrap_or(0)).unwrap_or(0) >= 2;

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
    use serde_json::json;

    #[test]
    fn merge_preserves_user_keys_and_env() {
        let raw = serde_json::to_string(&json!({
            "model": "claude-sonnet",
            "env": { "HTTP_PROXY": "http://127.0.0.1:7890", "ANTHROPIC_BASE_URL": "https://old.example.com" },
            "alwaysThinkingEnabled": true,
        }))
        .unwrap();

        let mut v: Value = serde_json::from_str(&raw).unwrap();
        let obj = v.as_object_mut().unwrap();
        obj.insert("model".into(), Value::String("deepseek-v4-pro".into()));
        let env = obj.entry("env").or_insert_with(|| Value::Object(Default::default()));
        let env_obj = env.as_object_mut().unwrap();
        env_obj.insert(ENV_BASE_URL.into(), Value::String("https://aigw.greenpool.cn".into()));
        env_obj.insert(ENV_API_KEY.into(), Value::String("sk-gp".into()));

        assert_eq!(v["model"], "deepseek-v4-pro");
        assert_eq!(v["env"][ENV_BASE_URL], "https://aigw.greenpool.cn");
        assert_eq!(v["env"]["HTTP_PROXY"], "http://127.0.0.1:7890"); // 用户 env 保留
        assert_eq!(v["alwaysThinkingEnabled"], true); // 其他顶层键保留
        assert!(ClaudeAdapter::is_managed(&v));
    }
}
