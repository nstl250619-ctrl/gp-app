//! 一键配置编排：探测 → 计划 → 备份写入校验 → 回滚 → 历史。
//! 记录暂用 app_data_dir 下的 JSON 文件（骨架期零新依赖；§12.1 的 SQLite 留待引入 rusqlite 时切换）。
//!
//! ## 还原原则（产品裁决 2026-10-18）
//! **还原 = 写回「执行本次配置之前」的备份文件内容**（即 GP 介入前的原始配置），
//! 而不是回退到「上一次 GP 写入后的状态」。落实方式：
//! 1. `apply` 仅在「当前配置未被 GP 托管」时创建备份（托管态不再新建 → 杜绝链式污染）；
//! 2. 托管态再次导入时，继承最近一次「GP 介入前」的还原点；
//! 3. `rollback` 写回前校验备份内容：若检测到 GP 托管标记（历史版本产生的污染快照）则
//!    拒绝还原并明确报错，绝不静默把 GP 配置再写一遍。

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::adapters::{self, types::{ApplyResult, DetectResult, InstallPlan, ModelSeed, WriteInput}};
use crate::config;
use crate::credential;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;

/// 写入互斥：apply/rollback 的「读-改-写」竞态防护（修 H5）。
static INSTALL_LOCK: Mutex<()> = Mutex::new(());

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstallRecord {
    pub id: String,
    pub target: String,
    /// 实际写入的配置路径。回滚按它还原，不重新探测（修 H2）。旧记录无此字段时回退探测。
    #[serde(default)]
    pub target_path: Option<String>,
    pub created_at: String,
    pub status: String,
    pub backup_path: Option<String>,
}

fn resolve_ids(model_ids: Vec<String>) -> Vec<String> {
    if model_ids.is_empty() {
        config::DEFAULT_MODEL_IDS.iter().map(|s| s.to_string()).collect()
    } else {
        model_ids
    }
}

fn seeds_for(model_ids: &[String]) -> Vec<ModelSeed> {
    model_ids
        .iter()
        .map(|id| ModelSeed {
            id: id.clone(),
            supports_tool_call: true,
            supports_images: true,
            supports_reasoning: true,
        })
        .collect()
}

/// 服务地址白名单：导入配置只允许绿池官方网关（防任意 URL 注入用户配置文件）。
const ALLOWED_BASE_URLS: [&str; 2] = ["https://api.greenpool.cn", "https://aigw.greenpool.cn"];

fn normalize_base_url(base_url: Option<String>) -> AppResult<String> {
    match base_url {
        None => Ok(config::DEFAULT_BASE_URL.into()),
        Some(u) => {
            let u = u.trim().trim_end_matches('/').to_string();
            if ALLOWED_BASE_URLS.contains(&u.as_str()) {
                Ok(u)
            } else {
                Err(AppError::Other("不支持的服务地址".into()))
            }
        }
    }
}

fn write_input(model_ids: Vec<String>, base_url: Option<String>) -> AppResult<WriteInput> {
    let api_key = credential::get(config::CREDENTIAL_ACCOUNT_API_KEY)?.ok_or(AppError::KeyMissing)?;
    Ok(WriteInput { base_url: normalize_base_url(base_url)?, api_key, model_ids })
}

/// 账号可用模型列表（分组过滤+去重），走网关 /api/user/models。
/// 拉取失败时回退硬编码默认清单（保底可用）。
pub async fn available_models() -> Vec<String> {
    match crate::services::account::ensure_session().await {
        Ok(_) => match crate::services::shop::gateway_models().await {
            Ok(models) if !models.is_empty() => models,
            _ => config::DEFAULT_MODEL_IDS.iter().map(|s| s.to_string()).collect(),
        },
        Err(_) => config::DEFAULT_MODEL_IDS.iter().map(|s| s.to_string()).collect(),
    }
}

pub fn detect(data_dir: &PathBuf) -> AppResult<Vec<DetectResult>> {
    let overrides = load_overrides(data_dir);
    adapters::all_adapters()
        .iter()
        .map(|a| a.detect(override_of(&overrides, a.id()).as_deref()))
        .collect()
}

pub fn plan(
    target: String,
    model_ids: Vec<String>,
    custom_path: Option<String>,
    base_url: Option<String>,
    data_dir: &PathBuf,
) -> AppResult<InstallPlan> {
    let ids = resolve_ids(model_ids);
    let input = write_input(ids.clone(), base_url)?;
    let adapter = adapters::find_adapter(&target).ok_or(AppError::TargetNotFound)?;
    let seeds = seeds_for(&ids);
    adapter.plan(&input, &seeds, resolve_custom(custom_path, data_dir, &target).as_deref())
}

fn apply_locked(
    target: String,
    model_ids: Vec<String>,
    custom_path: Option<String>,
    base_url: Option<String>,
    data_dir: &PathBuf,
) -> AppResult<ApplyResult> {
    let ids = resolve_ids(model_ids);
    let input = write_input(ids.clone(), base_url)?;
    let adapter = adapters::find_adapter(&target).ok_or(AppError::TargetNotFound)?;
    let seeds = seeds_for(&ids);
    let custom = resolve_custom(custom_path.clone(), data_dir, &target);
    let result = adapter.apply(&input, &seeds, custom.as_deref())?;

    // 用户手动指定过路径 → 持久化，detect/后续操作都指向它
    if let Some(p) = custom {
        save_override(data_dir, &target, &p.display().to_string())?;
    }

    let mut records = load_records(data_dir)?;

    // 托管态再次导入不产生新备份（adapter 跳过）→ 继承最近一次「GP 介入前」的还原点，
    // 保证任意一条记录的「还原」都回到 GP 写入前的原始配置。
    let mut backup_path = result.backup_path.clone();
    if backup_path.is_none() {
        backup_path = records
            .iter()
            .rev()
            .find(|r| r.target == target && r.status != "rolled_back")
            .and_then(|r| r.backup_path.clone());
    }

    // 备份镜像到 GP 数据目录：目标同目录的 .bak 文件可能被客户端应用清理（实测 Codex 会清 .codex 下陌生文件）。
    if let Some(bp) = &backup_path {
        mirror_backup(Path::new(bp), data_dir);
    }

    let record = InstallRecord {
        id: uuid::Uuid::new_v4().to_string(),
        target,
        target_path: Some(result.path.clone()),
        created_at: chrono::Local::now().to_rfc3339(),
        status: result.status.clone(),
        backup_path,
    };
    records.push(record);
    save_records(data_dir, &records)?;

    Ok(result)
}

/// 写入 + 可选连接自测（§7.2 verify 语义：连接失败不回滚、不报失败，只标注 connectivity_failed）。
/// 锁只在同步写入段持有，绝不跨 await（async 命令要求 Future: Send）。
pub async fn apply_with_verify(
    target: String,
    model_ids: Vec<String>,
    verify: bool,
    custom_path: Option<String>,
    base_url: Option<String>,
    data_dir: &PathBuf,
) -> AppResult<ApplyResult> {
    let mut result = {
        let _guard = lock()?;
        apply_locked(target, model_ids.clone(), custom_path, base_url.clone(), data_dir)?
    };

    if verify && result.verify_ok {
        let model = resolve_ids(model_ids).into_iter().next().unwrap_or_default();
        if let Ok(key) = crate::services::account::require_api_key() {
            // 连通自测走用户选定的服务地址（默认 api，选 aigw 则测 aigw）
            let client = crate::newapi::NewApiClient::new(&normalize_base_url(base_url)?);
            match client.verify_key(&key, &model).await {
                Ok(true) => {}
                Ok(false) => {
                    result.status = "connectivity_failed".into();
                    result.message = "已写入，但连接未通过".into();
                }
                Err(_) => {
                    result.status = "connectivity_failed".into();
                    result.message = "已写入，但无法完成连接自测（可能离线）".into();
                }
            }
        }
    }
    Ok(result)
}

pub fn rollback(record_id: String, data_dir: &PathBuf) -> AppResult<()> {
    let _guard = lock()?;
    let records = load_records(data_dir)?;
    let rec = records
        .iter()
        .find(|r| r.id == record_id)
        .cloned()
        .ok_or_else(|| AppError::Other("找不到这条记录".into()))?;
    if rec.status == "rolled_back" {
        return Err(AppError::RollbackFailed("该记录已还原过".into()));
    }
    let backup = rec.backup_path.clone().ok_or_else(|| AppError::RollbackFailed("该记录无备份".into()))?;
    let adapter = adapters::find_adapter(&rec.target).ok_or(AppError::TargetNotFound)?;

    // 同目录备份可能已被客户端应用清理 → 回退到 GP 数据目录里的镜像副本。
    let backup_path = PathBuf::from(&backup);
    let backup_path = if backup_path.exists() {
        backup_path
    } else {
        let mirror = backup_mirrors_dir(data_dir).join(
            backup_path.file_name().ok_or_else(|| AppError::RollbackFailed("备份路径非法".into()))?,
        );
        if mirror.exists() {
            mirror
        } else {
            return Err(AppError::RollbackFailed(format!(
                "备份文件已丢失（可能被客户端应用清理）：{}", backup
            )));
        }
    };

    // 目标路径：优先记录的原路径（修 H2：防多版本/移动后把 A 的备份写进 B）；
    // 旧记录无 target_path 时回退当前探测结果。
    let target_path = rec
        .target_path
        .clone()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            let overrides = load_overrides(data_dir);
            adapters::find_adapter(&rec.target)
                .and_then(|a| a.detect(override_of(&overrides, a.id()).as_deref()).ok())
                .and_then(|d| d.path)
                .map(PathBuf::from)
        })
        .ok_or_else(|| AppError::RollbackFailed("找不到目标配置路径".into()))?;

    // 还原原则：只允许写回「执行配置前」的原始备份。
    // 历史污染快照（内容已是 GP 托管态）直接拒绝，避免「还原了却还是 GP」的误导。
    let backup_content = fs_atomic::read_string(&backup_path)?;
    if backup_looks_managed(&rec.target, &backup_content) {
        return Err(AppError::RollbackFailed(
            "该还原点是旧版本产生的 GP 配置快照（非配置前原始备份），已阻止回退；请手工恢复原始配置或重新导入后再还原".into(),
        ));
    }

    adapter.rollback(&backup_path, &target_path)?;

    let mut records = records;
    if let Some(r) = records.iter_mut().find(|r| r.id == record_id) {
        r.status = "rolled_back".into();
    }
    save_records(data_dir, &records)
}

/// 操作记录：最新在最前（产品裁决 2026-10-13）。
pub fn history(data_dir: &PathBuf) -> AppResult<Vec<InstallRecord>> {
    let mut records = load_records(data_dir)?;
    records.reverse();
    Ok(records)
}

/// 备份内容是否已是「GP 托管态」——用于拦截历史版本产生的污染还原点。
/// 背景：2026-10-18 之前的版本在托管态重复导入时会备份「上一次 GP 写入后」的文件，
/// 这类快照写回后仍保留 GP 配置，违背「还原 = 回到配置前」原则。
fn backup_looks_managed(target: &str, content: &str) -> bool {
    match target {
        // WorkBuddy / CodeBuddy：托管条目 name 固定为 "GP"（MANAGED_MODEL_NAME）
        "workbuddy" | "codebuddy" => {
            content.contains("\"name\": \"GP\"") || content.contains("\"name\":\"GP\"")
        }
        // Codex：GP provider id 固定 "gp"
        "codex" => content.contains("model_provider") && content.contains("= \"gp\""),
        // Claude Code：ANTHROPIC_BASE_URL 指向 greenpool
        "claude" => content.contains("ANTHROPIC_BASE_URL") && content.contains("greenpool"),
        // OpenClaw / DSH / Hermes：provider 段含 greenpool
        "openclaw" | "dsh" | "hermes" => content.contains("greenpool") && content.contains("provider"),
        _ => false,
    }
}

/// 备份镜像目录：<data_dir>/backups/。
fn backup_mirrors_dir(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("backups")
}

/// 把同目录备份复制一份到 GP 数据目录（尽力而为，失败不阻断导入）。
fn mirror_backup(backup: &Path, data_dir: &PathBuf) {
    let Some(name) = backup.file_name() else { return };
    let dir = backup_mirrors_dir(data_dir);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::copy(backup, dir.join(name));
}

fn lock() -> AppResult<std::sync::MutexGuard<'static, ()>> {
    INSTALL_LOCK.lock().map_err(|_| AppError::Other("内部状态异常，请重启应用".into()))
}

fn records_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("install_records.json")
}

fn load_records(data_dir: &PathBuf) -> AppResult<Vec<InstallRecord>> {
    let p = records_path(data_dir);
    if !p.exists() {
        return Ok(Vec::new());
    }
    let raw = std::fs::read_to_string(&p)?;
    serde_json::from_str(&raw).map_err(|e| {
        AppError::Other(format!("安装记录文件损坏，无法读取（可删除后重试）：{}", e))
    })
}

/// 原子写（修 H5：崩溃安全，半写不会损坏记录文件）。
fn save_records(data_dir: &PathBuf, records: &[InstallRecord]) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    let content = serde_json::to_string_pretty(records)?;
    fs_atomic::atomic_write(&records_path(data_dir), &content)?;
    Ok(())
}

// ---- 用户手动指定的配置路径（分身/重命名场景）：按 target 持久化到 path_overrides.json ----

fn overrides_path(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("path_overrides.json")
}

fn load_overrides(data_dir: &PathBuf) -> std::collections::HashMap<String, String> {
    std::fs::read_to_string(overrides_path(data_dir))
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_default()
}

fn override_of(overrides: &std::collections::HashMap<String, String>, target: &str) -> Option<PathBuf> {
    overrides
        .get(target)
        .filter(|s| !s.trim().is_empty())
        .map(PathBuf::from)
}

fn save_override(data_dir: &PathBuf, target: &str, path: &str) -> AppResult<()> {
    std::fs::create_dir_all(data_dir)?;
    let mut m = load_overrides(data_dir);
    m.insert(target.to_string(), path.to_string());
    fs_atomic::atomic_write(&overrides_path(data_dir), &serde_json::to_string_pretty(&m)?)?;
    Ok(())
}

/// 自定义路径解析优先级：命令参数（用户输入框当前值） > 持久化 override > None（自动探测）
fn resolve_custom(custom_path: Option<String>, data_dir: &PathBuf, target: &str) -> Option<PathBuf> {
    let from_arg = custom_path
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .map(PathBuf::from);
    from_arg.or_else(|| {
        let overrides = load_overrides(data_dir);
        override_of(&overrides, target)
    })
}
