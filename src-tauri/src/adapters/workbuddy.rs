//! models.json 型客户端适配器（§11）：探测 / 读取 / 计划 / 备份 / 写入 / 回滚。
//! WorkBuddy 与 CodeBuddy 的配置文件同构（models 数组 + url/apiKey），共用同一套逻辑，
//! 仅目标标识与候选路径不同（产品裁决 2026-10-09：CodeBuddy 为第二个一键配置平台）。

use std::path::{Path, PathBuf};

use serde_json::Value;

use crate::config;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

use super::json_merge::{is_managed_entry, merge, normalize, rebuild_outer};
use super::models_template::{base_url_of, build_model_entry, resolve_related};
use super::types::{ApplyResult, DetectResult, DiffRow, InstallPlan, ModelSeed, WriteInput};

pub struct ModelsJsonAdapter {
    target: &'static str,
    display: &'static str,
    candidates: fn() -> Vec<PathBuf>,
}

impl ModelsJsonAdapter {
    fn existing_path(&self) -> Option<PathBuf> {
        (self.candidates)().into_iter().find(|p| p.exists())
    }

    pub fn workbuddy() -> Self {
        Self { target: "workbuddy", display: "WorkBuddy", candidates: config::workbuddy_candidate_paths }
    }

    pub fn codebuddy() -> Self {
        Self { target: "codebuddy", display: "CodeBuddy", candidates: config::codebuddy_candidate_paths }
    }
}

impl super::ConfigAdapter for ModelsJsonAdapter {
    fn id(&self) -> &'static str {
        self.target
    }

    fn display_name(&self) -> &'static str {
        self.display
    }

    fn detect(&self) -> AppResult<DetectResult> {
        let path = self.existing_path();
        let has_managed = match &path {
            Some(p) => {
                let raw = fs_atomic::read_string(p).unwrap_or_default();
                normalize(&raw).models.iter().any(is_managed_entry)
            }
            None => false,
        };
        let detected = path.is_some();
        let path_str = path.as_ref().map(|p| p.display().to_string());
        Ok(DetectResult {
            target: self.target.into(),
            display_name: self.display.into(),
            path: path_str,
            detected,
            has_managed_entry: has_managed,
        })
    }

    fn plan(&self, input: &WriteInput, seeds: &[ModelSeed]) -> AppResult<InstallPlan> {
        let path = self.existing_path().ok_or(AppError::TargetNotFound)?;
        let raw = fs_atomic::read_string(&path)?;
        let current = normalize(&raw);
        let (related, warnings) = resolve_related(&input.model_ids);
        let entries: Vec<Value> = seeds.iter().map(|s| build_model_entry(input, s, &related)).collect();
        let merged = merge(&current, entries);

        let before_managed = current.models.iter().filter(|e| is_managed_entry(e)).count();
        let preserved = merged.iter().filter(|e| !is_managed_entry(e)).count();
        let avail_desc = |n: usize| if n == 0 { "显示全部".to_string() } else { format!("限定 {} 项", n) };
        let after_avail = current.available_models.len();
        let diff = vec![
            DiffRow {
                field: "模型条目".into(),
                before: format!("{} 个", current.models.len()),
                after: format!("{} 个", merged.len()),
            },
            DiffRow {
                field: "下拉列表显示范围".into(),
                before: avail_desc(current.available_models.len()),
                after: avail_desc(after_avail),
            },
        ];

        // 对照表真实数据源：现有托管条目的 url/apiKey（明文为产品裁决：用户自己的配置文件本就是明文）
        let managed_now = current.models.iter().find(|e| is_managed_entry(e));
        let base_url_before = managed_now
            .and_then(|e| e.get("url"))
            .and_then(|u| u.as_str())
            .map(base_url_of);
        let key_before = managed_now
            .and_then(|e| e.get("apiKey"))
            .and_then(|k| k.as_str())
            .map(|s| s.to_string());

        Ok(InstallPlan {
            target: self.target.into(),
            path: path.display().to_string(),
            added: seeds.iter().map(|s| s.id.clone()).collect(),
            overwritten: before_managed,
            preserved,
            requires_restart: false,
            warnings,
            diff,
            related_lite: related.lite,
            related_reasoning: related.reasoning,
            base_url_before,
            base_url_after: input.base_url.clone(),
            key_before,
            key_after: input.api_key.clone(),
        })
    }

    fn apply(&self, input: &WriteInput, seeds: &[ModelSeed]) -> AppResult<ApplyResult> {
        let path = self.existing_path().ok_or(AppError::TargetNotFound)?;
        if let Some(parent) = path.parent() {
            if !parent.exists() {
                return Err(AppError::TargetNotFound); // 禁止静默创建目录（§11.1）
            }
        }

        // 备份失败即中止（AC-08 / §3.1-7）
        let backup = if path.exists() { Some(fs_atomic::backup(&path)?) } else { None };

        let raw = fs_atomic::read_string(&path)?;
        let current = normalize(&raw);
        let (related, _warnings) = resolve_related(&input.model_ids);
        let entries: Vec<Value> = seeds.iter().map(|s| build_model_entry(input, s, &related)).collect();
        let merged = merge(&current, entries);

        // 写前保留原 availableModels 原值，回读校验要对比（绝不主动收敛，§11.3）
        let avail_before: Option<Value> = serde_json::from_str::<Value>(&raw)
            .ok()
            .and_then(|v| v.get("availableModels").cloned());

        // key 级重建：只覆盖 models，其余顶层字段原样保留（修 H3）
        let outer = rebuild_outer(&raw, merged);
        let content = serde_json::to_string_pretty(&outer)?;

        fs_atomic::atomic_write(&path, &content)?;

        // 回读校验（AC-10）：解析通过 + 托管条目存在 + availableModels 与写前一致
        let back = fs_atomic::read_string(&path)?;
        let parsed: Value = serde_json::from_str(&back)?;
        let verify_models = parsed
            .get("models")
            .and_then(|m| m.as_array())
            .map(|arr| arr.iter().any(is_managed_entry))
            .unwrap_or(false);
        let verify_avail = match &avail_before {
            None => true,
            before => parsed.get("availableModels") == before.as_ref(),
        };
        let verify_ok = verify_models && verify_avail;

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
