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

    /// 回读校验：三个值必须与本次写入**逐一相等**。
    /// 只验「键存在」会在「写入失败但旧值还在」时误判成功（假成功）。
    fn verify_written(parsed: &Value, model_id: &str, input: &WriteInput) -> bool {
        parsed.get("model").and_then(|m| m.as_str()) == Some(model_id)
            && Self::env_field(parsed, ENV_BASE_URL) == Some(input.base_url.as_str())
            && Self::env_field(parsed, ENV_API_KEY) == Some(input.api_key.as_str())
    }

    /// 写入或校验失败时的收尾：有还原点就用它把文件恢复成写入前的样子，并把结果写进错误消息。
    /// 返回的是可直接展示给人看的后缀说明；还原失败时把备份路径一并给出，便于人工恢复。
    fn settle_failure(&self, backup: &Option<PathBuf>, path: &Path, reason: &str) -> String {
        match backup {
            Some(bak) => match super::ConfigAdapter::rollback(self, bak, path) {
                Ok(()) => format!("{}，已自动还原为写入前配置", reason),
                Err(e) => format!(
                    "{}，且自动还原失败（备份：{}）：{}",
                    reason,
                    bak.display(),
                    e
                ),
            },
            None => format!("{}（无可用还原点，请重试）", reason),
        }
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

        // ① 全部只读体检前置：任一项不通过都直接返回，磁盘上不多出任何文件（失败不留孤儿备份）。
        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(AppError::TargetNotFound); // 禁止静默创建目录（§11.1）
            }
        }

        let raw = fs_atomic::read_string(&path)?;
        let mut v: Value =
            serde_json::from_str(&raw).map_err(|e| AppError::Other(format!("settings.json 无法解析：{}", e)))?;
        if !v.is_object() {
            return Err(AppError::Other("settings.json 顶层必须是对象".into()));
        }
        if let Some(env) = v.get("env") {
            if !env.is_object() {
                return Err(AppError::Other("settings.json 的 env 必须是对象".into()));
            }
        }

        // ② 体检通过后才留还原点：已是 GP 托管态不新建备份（链式备份防护）。
        let managed = Self::is_managed(&v);
        let backup = if managed { None } else { Some(fs_atomic::backup(&path)?) };

        // ③ 至此才修改内存并落盘
        let obj = v.as_object_mut().expect("顶层对象已在 ① 校验");
        obj.insert("model".into(), Value::String(model_id.clone()));

        // env 深合并：只覆盖两个托管键，用户其他环境变量原样保留
        let env = obj
            .entry("env")
            .or_insert_with(|| Value::Object(Default::default()));
        let env_obj = env.as_object_mut().expect("env 对象已在 ① 校验");
        env_obj.insert(ENV_BASE_URL.into(), Value::String(input.base_url.clone()));
        env_obj.insert(ENV_API_KEY.into(), Value::String(input.api_key.clone()));

        let content = serde_json::to_string_pretty(&v)?;
        if let Err(e) = fs_atomic::atomic_write(&path, &content) {
            // 写盘失败时目标文件可能已被删除/截断，必须用还原点救回来，不能留半坏状态。
            return Err(AppError::Other(self.settle_failure(
                &backup,
                &path,
                &format!("写入失败：{}", e),
            )));
        }

        // 回读校验：回读出来的值必须与本次写入逐一相等（不是「键存在」就算过）。
        let back = fs_atomic::read_string(&path)?;
        let parsed: Value = serde_json::from_str(&back)?;
        if !Self::verify_written(&parsed, &model_id, input) {
            return Err(AppError::VerifyFailed(self.settle_failure(
                &backup,
                &path,
                "回读内容与预期不一致",
            )));
        }

        Ok(ApplyResult {
            extra_backups: Vec::new(),
            status: "applied".into(),
            path: path.display().to_string(),
            backup_path: backup.map(|b| b.display().to_string()),
            verify_ok: true,
            requires_restart: false,
            message: "已写入".into(),
        })
    }

    /// 按记录的目标路径还原（不重新探测），备份内容原子写回 + 摘要校验。
    fn is_managed_content(&self, raw: &str) -> bool {
        serde_json::from_str::<Value>(raw)
            .map(|v| Self::is_managed(&v))
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
    use crate::adapters::ConfigAdapter;
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

    fn seed(id: &str) -> ModelSeed {
        ModelSeed {
            id: id.into(),
            supports_tool_call: true,
            supports_images: false,
            supports_reasoning: true,
        }
    }

    fn input() -> WriteInput {
        WriteInput {
            base_url: "https://aigw.greenpool.cn".into(),
            api_key: "sk-gp".into(),
            model_ids: vec!["deepseek-v4-pro".into()],
        }
    }

    /// 体检失败分支：目录里不能多出任何文件（孤儿备份），原文件也必须一字未改。
    #[test]
    fn failed_validation_leaves_no_backup() {
        let cases: [(&str, &str, Vec<ModelSeed>); 3] = [
            (r#"{"model":"claude-sonnet","env":"oops"}"#, "env 必须是对象", vec![seed("m1")]),
            (r#"["not","an","object"]"#, "顶层必须是对象", vec![seed("m1")]),
            (r#"{"model":"claude-sonnet"}"#, "未选择模型", Vec::new()),
        ];

        for (original, expect_msg, seeds) in cases {
            let dir = tempfile::tempdir().unwrap();
            let path = dir.path().join("settings.json");
            std::fs::write(&path, original).unwrap();

            let err = ClaudeAdapter::new()
                .apply(&input(), &seeds, Some(&path))
                .expect_err("校验不通过必须返回错误");
            assert!(
                err.to_string().contains(expect_msg),
                "错误消息应包含 `{}`，实际：{}",
                expect_msg,
                err
            );

            let extra: Vec<String> = std::fs::read_dir(dir.path())
                .unwrap()
                .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
                .filter(|n| n != "settings.json")
                .collect();
            assert!(extra.is_empty(), "失败分支不应留下文件，实际多出：{:?}", extra);
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "失败后原文件不能变");
        }
    }

    /// 成功分支：留还原点、两个托管键写入、用户键全部保留。
    #[test]
    fn apply_creates_backup_and_preserves_user_config() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            serde_json::to_string(&json!({
                "model": "claude-sonnet",
                "env": { "HTTP_PROXY": "http://127.0.0.1:7890" },
                "alwaysThinkingEnabled": true,
            }))
            .unwrap(),
        )
        .unwrap();

        let res = ClaudeAdapter::new()
            .apply(&input(), &[seed("deepseek-v4-pro")], Some(&path))
            .expect("正常写入不应失败");

        assert_eq!(res.status, "applied");
        assert!(res.verify_ok);
        assert!(res.backup_path.is_some(), "首次接管必须留还原点");

        let v: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(v["model"], "deepseek-v4-pro");
        assert_eq!(v["env"][ENV_BASE_URL], "https://aigw.greenpool.cn");
        assert_eq!(v["env"][ENV_API_KEY], "sk-gp");
        assert_eq!(v["env"]["HTTP_PROXY"], "http://127.0.0.1:7890");
        assert_eq!(v["alwaysThinkingEnabled"], true);
        assert!(ClaudeAdapter::is_managed(&v));
    }

    /// 已是托管态再写：不再链式生成新备份（还原点始终是「GP 介入前」的原始配置）。
    #[test]
    fn reapply_when_managed_does_not_chain_backups() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, r#"{"env":{"ANTHROPIC_BASE_URL":"https://aigw.greenpool.cn"}}"#).unwrap();

        let res = ClaudeAdapter::new()
            .apply(&input(), &[seed("deepseek-v4-pro")], Some(&path))
            .expect("托管态写入不应失败");

        assert!(res.verify_ok);
        assert!(res.backup_path.is_none(), "托管态不应新建备份");
    }

    /// 第 1 项：回读校验必须比值，键存在但值还是旧的必须判失败。
    #[test]
    fn verify_rejects_old_value_even_if_key_present() {
        let inp = input();

        let good = json!({
            "model": "deepseek-v4-pro",
            "env": { "ANTHROPIC_BASE_URL": "https://aigw.greenpool.cn", "ANTHROPIC_API_KEY": "sk-gp" }
        });
        assert!(ClaudeAdapter::verify_written(&good, "deepseek-v4-pro", &inp));

        // 三个键都在，但 base_url 还是旧地址 —— 旧实现只看 is_some()，会误判成成功
        let stale = json!({
            "model": "deepseek-v4-pro",
            "env": { "ANTHROPIC_BASE_URL": "https://old.example.com", "ANTHROPIC_API_KEY": "sk-gp" }
        });
        assert!(!ClaudeAdapter::verify_written(&stale, "deepseek-v4-pro", &inp));

        // 密钥根本没写进去
        let no_key = json!({
            "model": "deepseek-v4-pro",
            "env": { "ANTHROPIC_BASE_URL": "https://aigw.greenpool.cn" }
        });
        assert!(!ClaudeAdapter::verify_written(&no_key, "deepseek-v4-pro", &inp));
    }

    /// 第 2 项：失败收尾必须把文件还原成写入前的样子，并保留备份供人工复核。
    #[test]
    fn settle_failure_restores_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let original = r#"{"model":"claude-sonnet","env":{"HTTP_PROXY":"http://127.0.0.1:7890"}}"#;
        std::fs::write(&path, original).unwrap();
        let bak = fs_atomic::backup(&path).unwrap();
        std::fs::write(&path, r#"{"model":"half-written"}"#).unwrap(); // 模拟写坏

        let msg = ClaudeAdapter::new().settle_failure(&Some(bak.clone()), &path, "回读内容与预期不一致");

        assert!(msg.contains("已自动还原"), "消息应说明已还原：{}", msg);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), original, "文件必须回到写入前内容");
        assert!(bak.exists(), "备份要留着供人工复核");
    }

    #[test]
    fn settle_failure_without_backup_reports_it() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(&path, "{}").unwrap();

        let msg = ClaudeAdapter::new().settle_failure(&None, &path, "写入失败：磁盘已满");

        assert!(msg.contains("无可用还原点"), "实际：{}", msg);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "{}", "无备份时不要动文件");
    }
}
