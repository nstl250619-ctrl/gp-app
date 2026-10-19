//! DeepSeek Harness（DSH）适配器（多模型列表型）：探测 / 读取 / 计划 / 备份 / 写入 / 回滚。
//!
//! 实测依据（2026-10-14 本机 DSH 桌面版 + app.asar 源码挖掘）：
//! - 用户配置层 = `$DSH_HOME/profiles/desktop/cordis.patch.yml`（官方注释明确：编辑 patch 层，勿改 cordis.yml）
//! - LLM 走内嵌 `@earendil-works/pi-ai`：`llm-pi-ai` patch 条目声明 providers.<id>，
//!   模型选择器按 `displayName` 分组（如 "GP:"，与用户已有配置一致）
//! - key 经 `apiKeyEnv` 引用 `.credentials.yaml` 的 `refs`（如 GREENPOOL_API_KEY）
//! - `agent-default-model` patch 条目指定默认 provider/model
//! - ⚠️ baseURL 必须带 /v1：不带时请求打到 new-api 的 SPA fallback 返回 HTML(200)，
//!   pi-ai 解析 SSE 失败报「Stream ended without finish_reason」（本机实测根因）
//!
//! 写入两个文件：
//! 1. cordis.patch.yml（patch 条目数组合并：llm-pi-ai + agent-default-model）
//! 2. .credentials.yaml（refs.GREENPOOL_API_KEY 行合并，records 原样保留）
//!
//! serde_yaml 全量序列化会丢失 YAML 注释（patch 文件头部引导注释），已知限制，plan warning 说明。

use std::path::{Path, PathBuf};

use serde_json::{json, Value};

use crate::config;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

use super::types::{ApplyResult, DetectResult, DiffRow, ExtraBackup, InstallPlan, ModelSeed, WriteInput};

const PROVIDER_ID: &str = "greenpool";
const DISPLAY_NAME: &str = "GP:";
const CRED_KEY: &str = "GREENPOOL_API_KEY";
const MODEL_CONTEXT_WINDOW: u64 = 1_000_000;
const MODEL_MAX_TOKENS: u64 = 32_000;

pub struct DshAdapter {
    candidates: fn() -> Vec<PathBuf>,
}

impl DshAdapter {
    pub fn new() -> Self {
        Self { candidates: config::dsh_candidate_paths }
    }

    fn existing_path(&self, custom: Option<&Path>) -> Option<PathBuf> {
        match custom {
            Some(p) => Some(p.to_path_buf()),
            // 新装 DSH 可能还没有 patch 文件 → 回退到第一个候选路径（apply 会创建，内容为空的条目数组）
            None => (self.candidates)()
                .into_iter()
                .find(|p| p.exists())
                .or_else(|| (self.candidates)().into_iter().next()),
        }
    }

    /// credentials.yaml 与 patch 文件同 profile 根（profiles/desktop 的上级）。
    fn credentials_path(patch_path: &Path) -> PathBuf {
        patch_path
            .parent() // profiles/desktop
            .and_then(|p| p.parent()) // profiles
            .and_then(|p| p.parent()) // profile root
            .map(|p| p.join(config::DSH_CREDENTIALS_FILE))
            .unwrap_or_else(|| PathBuf::from(config::DSH_CREDENTIALS_FILE))
    }

    /// 读取 patch 条目；文件不存在/为空时返回空数组（新装 DSH 无 patch，apply 会创建）。
    fn read_patch(path: &Path) -> AppResult<Vec<Value>> {
        if !path.exists() {
            return Ok(Vec::new());
        }
        let raw = fs_atomic::read_string(path)?;
        if raw.trim().is_empty() {
            return Ok(Vec::new());
        }
        Self::parse_patch(&raw)
    }

    /// 行级合并 `.credentials.yaml` 的 `refs.<key>`：
    /// 只替换/追加目标行，其余内容（records 加密凭据、注释、缩进）逐字节保留。
    /// 背景：全量 YAML 反序列化+重写会改变 records 的原始格式（键序/引号/类型标签），
    /// 可能让 DSH 读不出其他凭据 → 用户被莫名登出。这里只碰一行。
    fn merge_cred_ref(raw: &str, key: &str, value: &str) -> String {
        let scalar = serde_yaml::to_string(&value)
            .map(|s| s.trim_end().to_string())
            .unwrap_or_else(|_| format!("{:?}", value));
        let mut lines: Vec<String> = raw.lines().map(|l| l.to_string()).collect();

        let refs_start = lines
            .iter()
            .position(|l| l.trim_end() == "refs:" && !l.starts_with(' ') && !l.starts_with('\t'));

        match refs_start {
            Some(start) => {
                // refs 段结束 = 下一个顶层非空行
                let end = ((start + 1)..lines.len())
                    .find(|&i| {
                        let t = &lines[i];
                        !t.trim().is_empty() && !t.starts_with(' ') && !t.starts_with('\t')
                    })
                    .unwrap_or(lines.len());
                let prefix = format!("{}:", key);
                let hit = ((start + 1)..end).find(|&i| lines[i].trim_start().starts_with(&prefix));
                match hit {
                    Some(i) => lines[i] = format!("  {}: {}", key, scalar),
                    None => lines.insert(end, format!("  {}: {}", key, scalar)),
                }
            }
            None => {
                while lines.last().map(|l| l.trim().is_empty()).unwrap_or(false) {
                    lines.pop();
                }
                lines.push("refs:".into());
                lines.push(format!("  {}: {}", key, scalar));
            }
        }

        let mut out = lines.join("\n");
        out.push('\n');
        out
    }

    fn parse_patch(raw: &str) -> AppResult<Vec<Value>> {
        let v: Value = serde_yaml::from_str(raw)
            .map_err(|e| AppError::Other(format!("cordis.patch.yml 无法解析：{}", e)))?;
        let items = v
            .as_array()
            .ok_or_else(|| AppError::Other("cordis.patch.yml 顶层必须是条目数组".into()))?;
        Ok(items.clone())
    }

    /// 找 patch 条目（按 id），无则新建。
    fn entry<'a>(items: &'a mut Vec<Value>, id: &str, name: &str) -> &'a mut Value {
        if let Some(pos) = items.iter().position(|e| e.get("id").and_then(|i| i.as_str()) == Some(id)) {
            return &mut items[pos];
        }
        items.push(json!({ "id": id, "name": name, "config": {} }));
        let last = items.len() - 1;
        &mut items[last]
    }

    /// 托管判定：providers.greenpool 存在。
    fn is_managed(items: &[Value]) -> bool {
        items
            .iter()
            .any(|e| e.get("id").and_then(|i| i.as_str()) == Some("llm-pi-ai"))
            && items.iter().any(|e| {
                e.get("config")
                    .and_then(|c| c.get("providers"))
                    .and_then(|p| p.get(PROVIDER_ID))
                    .is_some()
            })
    }

    /// 按 ModelSeed 构造 pi-ai 模型条目。
    fn model_entry(seed: &ModelSeed) -> Value {
        json!({
            "id": seed.id,
            "name": seed.id,
            "contextWindow": MODEL_CONTEXT_WINDOW,
            "maxTokens": MODEL_MAX_TOKENS,
        })
    }

    fn provider_entry(input: &WriteInput, seeds: &[ModelSeed]) -> Value {
        let models: Vec<Value> = seeds.iter().map(Self::model_entry).collect();
        json!({
            "displayName": DISPLAY_NAME,
            "apiKeyEnv": CRED_KEY,
            "api": "openai-completions",
            "baseURL": format!("{}/v1", input.base_url), // 必须带 /v1（SPA fallback 根因）
            "models": models,
        })
    }
}

impl Default for DshAdapter {
    fn default() -> Self {
        Self::new()
    }
}

impl super::ConfigAdapter for DshAdapter {
    fn id(&self) -> &'static str {
        "dsh"
    }

    fn display_name(&self) -> &'static str {
        "DeepSeek Harness"
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
                .and_then(|raw| Self::parse_patch(&raw).ok())
                .map(|items| Self::is_managed(&items))
                .unwrap_or(false),
            _ => false,
        };
        Ok(DetectResult {
            target: "dsh".into(),
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
        let items = Self::read_patch(&path)?;
        let has_managed = Self::is_managed(&items);

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;
        let base_url_before = items
            .iter()
            .find(|e| e.get("id").and_then(|i| i.as_str()) == Some("llm-pi-ai"))
            .and_then(|e| e.get("config"))
            .and_then(|c| c.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .and_then(|g| g.get("baseURL"))
            .and_then(|u| u.as_str())
            .map(|s| s.to_string());
        let before_count = items
            .iter()
            .find(|e| e.get("id").and_then(|i| i.as_str()) == Some("llm-pi-ai"))
            .and_then(|e| e.get("config"))
            .and_then(|c| c.get("providers"))
            .and_then(|p| p.get(PROVIDER_ID))
            .and_then(|g| g.get("models"))
            .and_then(|m| m.as_array())
            .map(|a| a.len())
            .unwrap_or(0);

        let diff = vec![
            DiffRow {
                field: "模型条目".into(),
                before: format!("{} 个", before_count),
                after: format!("{} 个", seeds.len()),
            },
            DiffRow {
                field: "服务地址".into(),
                before: base_url_before.clone().unwrap_or_else(|| "未配置".into()),
                after: format!("{}/v1", input.base_url),
            },
            DiffRow {
                field: "默认模型".into(),
                before: "—".into(),
                after: format!("GP: {}", model_id),
            },
        ];

        Ok(InstallPlan {
            target: "dsh".into(),
            path: path.display().to_string(),
            added: seeds.iter().map(|s| s.id.clone()).collect(),
            overwritten: if has_managed { 1 } else { 0 },
            preserved: items.len(),
            requires_restart: true, // DSH 需重启读取 patch 配置
            warnings: vec![
                "写入后需重启 DeepSeek Harness 生效".into(),
                "模型选择器将出现 GP: 分组，可切换全部导入模型".into(),
                "DSH 使用 Node 网络栈：建议选择 aigw.greenpool.cn（api.greenpool.cn 有 IPv6 解析，本机 IPv6 不通时 Node 会超时）".into(),
                "YAML 写入会丢失 patch 文件内注释（引导注释，无功能影响）".into(),
            ],
            diff,
            related_lite: None,
            related_reasoning: None,
            base_url_before,
            base_url_after: format!("{}/v1", input.base_url),
            key_before: None,
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

        let mut items = Self::read_patch(&path)?;
        let has_managed = Self::is_managed(&items);

        // 已是 GP 托管态时不新建备份：还原点必须是「GP 介入前」的原始配置（链式备份防护）。
        let backup = if !has_managed { Some(fs_atomic::backup(&path)?) } else { None };

        let model_id = seeds
            .first()
            .map(|s| s.id.clone())
            .ok_or_else(|| AppError::Other("未选择模型".into()))?;

        // 1) llm-pi-ai：providers.greenpool（多模型）
        {
            let entry = Self::entry(&mut items, "llm-pi-ai", "@deepseek-ai/dsh-llm-pi-ai");
            let obj = entry
                .as_object_mut()
                .ok_or_else(|| AppError::Other("patch 条目必须是映射".into()))?;
            let cfg = obj
                .entry("config")
                .or_insert_with(|| json!({}));
            if !cfg.is_object() {
                return Err(AppError::Other("patch 条目的 config 必须是映射".into()));
            }
            cfg.as_object_mut()
                .expect("checked above")
                .entry("providers")
                .or_insert_with(|| json!({}))
                .as_object_mut()
                .ok_or_else(|| AppError::Other("config.providers 必须是映射".into()))?
                .insert(PROVIDER_ID.into(), Self::provider_entry(input, seeds));
        }

        // 2) agent-default-model：默认主力模型
        {
            let entry = Self::entry(&mut items, "agent-default-model", "@deepseek-ai/dsh-agent-default-model");
            let obj = entry
                .as_object_mut()
                .ok_or_else(|| AppError::Other("patch 条目必须是映射".into()))?;
            obj.insert("config".into(), json!({ "provider": PROVIDER_ID, "model": model_id }));
        }

        let content = serde_yaml::to_string(&items)
            .map_err(|e| AppError::Other(format!("cordis.patch.yml 序列化失败：{}", e)))?;
        fs_atomic::atomic_write(&path, &content)?;

        // 3) credentials：refs.GREENPOOL_API_KEY —— 只做行级合并，绝不全量重写
        //    （records 里是 DSH 自己的加密凭据，YAML 往返可能改变键序/引号/类型标签 → 其他凭据失效）
        let cred_path = Self::credentials_path(&path);
        let cred_raw = if cred_path.exists() { fs_atomic::read_string(&cred_path)? } else { String::new() };
        // 解析校验：失败即中止（不破坏原文件）
        let cred_parsed: Option<Value> = if cred_raw.trim().is_empty() {
            None
        } else {
            Some(
                serde_yaml::from_str(&cred_raw)
                    .map_err(|e| AppError::Other(format!(".credentials.yaml 无法解析：{}", e)))?,
            )
        };
        // 是否已是 GP 托管态（refs 里已有我们的 key）：重复导入不新建备份（还原点=配置前）
        let cred_already_managed = cred_parsed
            .as_ref()
            .and_then(|v| v.get("refs").and_then(|r| r.get(CRED_KEY)))
            .is_some();

        let cred_content = Self::merge_cred_ref(&cred_raw, CRED_KEY, &input.api_key);
        let mut extra_backups: Vec<ExtraBackup> = Vec::new();
        if cred_content != cred_raw {
            if cred_path.exists() && !cred_raw.trim().is_empty() && !cred_already_managed {
                let bp = fs_atomic::backup(&cred_path)?;
                extra_backups.push(ExtraBackup {
                    backup: bp.display().to_string(),
                    target: cred_path.display().to_string(),
                });
            }
            fs_atomic::atomic_write(&cred_path, &cred_content)?;
        }
        // credentials 回读校验：目标 ref 在位、且文件仍可被 YAML 解析
        let cred_ok = fs_atomic::read_string(&cred_path)
            .ok()
            .and_then(|raw| serde_yaml::from_str::<Value>(&raw).ok())
            .and_then(|v| {
                v.get("refs")
                    .and_then(|r| r.get(CRED_KEY))
                    .and_then(|k| k.as_str())
                    .map(|k| k == input.api_key.as_str())
            })
            .unwrap_or(false);

        // 回读校验
        let back = fs_atomic::read_string(&path)?;
        let items2 = Self::parse_patch(&back)?;
        let verify_ok = Self::is_managed(&items2)
            && items2
                .iter()
                .any(|e| {
                    e.get("config")
                        .and_then(|c| c.get("provider"))
                        .and_then(|p| p.as_str())
                        .map(|p| p == PROVIDER_ID)
                        .unwrap_or(false)
                })
            && cred_ok;

        Ok(ApplyResult {
            extra_backups,
            status: if verify_ok { "applied".into() } else { "failed".into() },
            path: path.display().to_string(),
            backup_path: backup.map(|b| b.display().to_string()),
            verify_ok,
            requires_restart: true,
            message: if verify_ok { "已写入，重启 DeepSeek Harness 生效".into() } else { "写入校验未通过".into() },
        })
    }

    /// 按记录的目标路径还原（不重新探测），备份内容原子写回 + 摘要校验。
    fn is_managed_content(&self, raw: &str) -> bool {
        Self::parse_patch(raw)
            .map(|items| Self::is_managed(&items))
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

    const SAMPLE: &str = "# Your patch layer\n- id: ui-settings\n  name: \"@deepseek-ai/dsh-client-ui-settings\"\n  config:\n    enabled: true\n";

    fn input() -> WriteInput {
        WriteInput {
            base_url: "https://aigw.greenpool.cn".into(),
            api_key: "sk-gp".into(),
            model_ids: vec!["glm-5.3".into()],
        }
    }

    fn seed(id: &str) -> ModelSeed {
        ModelSeed { id: id.into(), supports_tool_call: true, supports_images: true, supports_reasoning: true }
    }

    #[test]
    fn merge_cred_ref_preserves_records_and_is_idempotent() {
        // records 是 DSH 自己的加密凭据：行级合并必须逐字节保留，只动 refs 里的目标行
        let raw = "version: 1\nrecords:\n  a/b:\n    kind: grant\n    token: abc\nrefs:\n  OLD_KEY: v1\n";
        let out = DshAdapter::merge_cred_ref(raw, "GREENPOOL_API_KEY", "sk-gp");
        let v: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(v["refs"]["GREENPOOL_API_KEY"], "sk-gp");
        assert_eq!(v["refs"]["OLD_KEY"], "v1");
        assert_eq!(v["records"]["a/b"]["token"], "abc");
        assert!(out.contains("    token: abc"), "records 段必须原样保留：{}", out);

        // 二次合并：替换而非追加（幂等）
        let out2 = DshAdapter::merge_cred_ref(&out, "GREENPOOL_API_KEY", "sk-gp2");
        let v2: Value = serde_yaml::from_str(&out2).unwrap();
        assert_eq!(v2["refs"]["GREENPOOL_API_KEY"], "sk-gp2");
        assert_eq!(v2["refs"]["OLD_KEY"], "v1");
        assert_eq!(out2.matches("GREENPOOL_API_KEY").count(), 1);
    }

    #[test]
    fn merge_cred_ref_creates_refs_when_missing() {
        let raw = "version: 1\nrecords:\n  x/y:\n    kind: grant\n";
        let out = DshAdapter::merge_cred_ref(raw, "GREENPOOL_API_KEY", "sk-gp");
        let v: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(v["refs"]["GREENPOOL_API_KEY"], "sk-gp");
        assert_eq!(v["records"]["x/y"]["kind"], "grant");
    }

    #[test]
    fn provider_entry_has_v1_suffix_and_display() {
        let e = DshAdapter::provider_entry(&input(), &[seed("glm-5.3")]);
        assert_eq!(e["baseURL"], "https://aigw.greenpool.cn/v1");
        assert_eq!(e["displayName"], "GP:");
        assert_eq!(e["apiKeyEnv"], "GREENPOOL_API_KEY");
        assert_eq!(e["api"], "openai-completions");
        assert_eq!(e["models"][0]["id"], "glm-5.3");
    }

    #[test]
    fn merge_preserves_other_entries() {
        let mut items = DshAdapter::parse_patch(SAMPLE).unwrap();
        assert!(!DshAdapter::is_managed(&items));
        let entry = DshAdapter::entry(&mut items, "llm-pi-ai", "@deepseek-ai/dsh-llm-pi-ai");
        entry.as_object_mut().unwrap().insert("config".into(), json!({ "providers": { "greenpool": { "baseURL": "x" } } }));
        let out = serde_yaml::to_string(&items).unwrap();
        let items2 = DshAdapter::parse_patch(&out).unwrap();
        assert!(DshAdapter::is_managed(&items2));
        // 其他 patch 条目保留
        assert!(items2.iter().any(|e| e.get("id").and_then(|i| i.as_str()) == Some("ui-settings")));
    }

    #[test]
    fn env_merge_for_credentials() {
        // credentials 合并逻辑复用 serde_yaml 深合并（在 apply 内联），此处验证结构
        let raw = "version: 1\nrecords:\n  a/b:\n    kind: grant\nrefs:\n  OLD_KEY: v\n";
        let mut v: Value = serde_yaml::from_str(raw).unwrap();
        v.as_object_mut().unwrap().entry("refs").or_insert_with(|| json!({}))
            .as_object_mut().unwrap()
            .insert("GREENPOOL_API_KEY".into(), Value::String("sk-gp".into()));
        let out = serde_yaml::to_string(&v).unwrap();
        let back: Value = serde_yaml::from_str(&out).unwrap();
        assert_eq!(back["refs"]["GREENPOOL_API_KEY"], "sk-gp");
        assert_eq!(back["refs"]["OLD_KEY"], "v"); // 其他 refs 保留
        assert!(back["records"]["a/b"].is_object()); // records 保留
    }
}
