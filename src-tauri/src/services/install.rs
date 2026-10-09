//! 一键配置编排：探测 → 计划 → 备份写入校验 → 回滚 → 历史。
//! 记录暂用 app_data_dir 下的 JSON 文件（骨架期零新依赖；§12.1 的 SQLite 留待引入 rusqlite 时切换）。

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

fn write_input(model_ids: Vec<String>) -> AppResult<WriteInput> {
    let api_key = credential::get(config::CREDENTIAL_ACCOUNT_API_KEY)?.ok_or(AppError::KeyMissing)?;
    Ok(WriteInput { base_url: config::DEFAULT_BASE_URL.into(), api_key, model_ids })
}

pub fn detect() -> AppResult<Vec<DetectResult>> {
    adapters::all_adapters().iter().map(|a| a.detect()).collect()
}

pub fn plan(target: String, model_ids: Vec<String>) -> AppResult<InstallPlan> {
    let ids = resolve_ids(model_ids);
    let input = write_input(ids.clone())?;
    let adapter = adapters::find_adapter(&target).ok_or(AppError::TargetNotFound)?;
    let seeds = seeds_for(&ids);
    adapter.plan(&input, &seeds)
}

fn apply_locked(target: String, model_ids: Vec<String>, data_dir: &PathBuf) -> AppResult<ApplyResult> {
    let ids = resolve_ids(model_ids);
    let input = write_input(ids.clone())?;
    let adapter = adapters::find_adapter(&target).ok_or(AppError::TargetNotFound)?;
    let seeds = seeds_for(&ids);
    let result = adapter.apply(&input, &seeds)?;

    let record = InstallRecord {
        id: uuid::Uuid::new_v4().to_string(),
        target,
        target_path: Some(result.path.clone()),
        created_at: chrono::Local::now().to_rfc3339(),
        status: result.status.clone(),
        backup_path: result.backup_path.clone(),
    };
    append_record(data_dir, record)?;

    Ok(result)
}

/// 写入 + 可选连接自测（§7.2 verify 语义：连接失败不回滚、不报失败，只标注 connectivity_failed）。
/// 锁只在同步写入段持有，绝不跨 await（async 命令要求 Future: Send）。
pub async fn apply_with_verify(
    target: String,
    model_ids: Vec<String>,
    verify: bool,
    data_dir: &PathBuf,
) -> AppResult<ApplyResult> {
    let mut result = {
        let _guard = lock()?;
        apply_locked(target, model_ids.clone(), data_dir)?
    };

    if verify && result.verify_ok {
        let model = resolve_ids(model_ids).into_iter().next().unwrap_or_default();
        if let Ok(key) = crate::services::account::require_api_key() {
            let client = crate::newapi::NewApiClient::default_client();
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

    // 目标路径：优先记录的原路径（修 H2：防多版本/移动后把 A 的备份写进 B）；
    // 旧记录无 target_path 时回退当前探测结果。
    let target_path = rec
        .target_path
        .clone()
        .filter(|p| !p.is_empty())
        .map(PathBuf::from)
        .or_else(|| {
            adapters::find_adapter(&rec.target)
                .and_then(|a| a.detect().ok())
                .and_then(|d| d.path)
                .map(PathBuf::from)
        })
        .ok_or_else(|| AppError::RollbackFailed("找不到目标配置路径".into()))?;

    adapter.rollback(Path::new(&backup), &target_path)?;

    let mut records = records;
    if let Some(r) = records.iter_mut().find(|r| r.id == record_id) {
        r.status = "rolled_back".into();
    }
    save_records(data_dir, &records)
}

pub fn history(data_dir: &PathBuf) -> AppResult<Vec<InstallRecord>> {
    load_records(data_dir)
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

fn append_record(data_dir: &PathBuf, record: InstallRecord) -> AppResult<()> {
    let mut records = load_records(data_dir)?;
    records.push(record);
    save_records(data_dir, &records)
}
