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

use crate::adapters::{self, types::{ApplyResult, DetectResult, ExtraBackup, InstallPlan, ModelSeed, WriteInput}};
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
    /// 附加文件备份（一个 target 写多个文件时，如 DSH 的 .credentials.yaml），随主备份一起还原。
    #[serde(default)]
    pub extra_backups: Vec<ExtraBackup>,
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

/// Node 网络栈的客户端：没有 Happy Eyeballs（IPv6 失败不会回落 IPv4）。
/// api.greenpool.cn 有 AAAA 记录，在「IPv6 半通」网络（手机热点/双层 NAT 宽带）会卡死超时；
/// aigw 只有 A 记录且是同一网关，因此这些客户端统一写 aigw（功能等价、路径更稳）。
const NODE_STACK_TARGETS: [&str; 4] = ["dsh", "claude", "openclaw", "hermes"];

/// 隧道是否处于「已连接」：仅在 connected 态做 base_url 静默替换（degraded/reconnecting 不替换，行为可预期）。
fn tunnel_connected(data_dir: &PathBuf) -> bool {
    matches!(
        crate::services::tunnel::status(data_dir).phase,
        crate::services::tunnel::TunnelPhase::Connected
    )
}

fn normalize_base_url(base_url: Option<String>, target: &str, data_dir: &PathBuf) -> AppResult<String> {
    // 隧道加速（产品决策）：隧道运行中时，所有 target 的 base_url 统一用本地转发地址。
    // 前端服务地址下拉照旧显示官方域名，此处对用户透明替换。
    if tunnel_connected(data_dir) {
        return Ok(config::TUNNEL_BASE_URL.into());
    }
    if NODE_STACK_TARGETS.contains(&target) {
        return Ok(config::PROBE_BASE_URL.into());
    }
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

/// 按 target 取实际使用的 API key：DSH 卡用专属 key，其他用通用 key。
/// 写入（write_input）与连通自测（apply 的 verify）必须用同一把——否则自测会拿错分组的 key 误报「连接未通过」。
fn api_key_for(target: &str) -> AppResult<String> {
    if target == "dsh" {
        // DSH 专属 key 缺失直接报错，不静默回退通用 key（否则 UI 锁定显示 DSH-Only 却写入通用 key，破坏隔离）。
        credential::get(config::CREDENTIAL_DSH_API_KEY)?
            .ok_or_else(|| AppError::Other("DSH 专属密钥未就绪，请重新登录后再试".into()))
    } else {
        credential::get(config::CREDENTIAL_ACCOUNT_API_KEY)?.ok_or(AppError::KeyMissing)
    }
}

/// 把网关自测错误翻译成简短的用户可读文案（去掉 request id 等冗余、映射常见错误）。
fn friendly_verify_error(raw: &str) -> String {
    let core = raw.split(" (request id").next().unwrap_or(raw).trim();
    let lowered = core.to_lowercase();
    if lowered.contains("quota") {
        "账号额度不足，请先充值或兑换后再试".into()
    } else if lowered.contains("invalid token") {
        "密钥无效或已失效，请重新登录后再试".into()
    } else if lowered.contains("no available channel") {
        "该模型暂无可用渠道".into()
    } else if lowered.contains("channel disabled") {
        "该渠道已停用".into()
    } else {
        core.to_string()
    }
}

fn write_input(model_ids: Vec<String>, base_url: Option<String>, target: &str, data_dir: &PathBuf) -> AppResult<WriteInput> {
    let api_key = api_key_for(target)?;
    Ok(WriteInput { base_url: normalize_base_url(base_url, target, data_dir)?, api_key, model_ids })
}

/// 账号可用模型列表（分组过滤+去重），走网关 /api/user/models。
/// DSH 卡按专属分组（DSH-Only）拉取；其他卡按账号级（不带 group）。
/// 返回 (models, offline)：offline=true 表示网关拉取失败、回退离线清单（可能过期，UI 需提示）。
pub async fn available_models(target: &str) -> (Vec<String>, bool) {
    let fallback = || config::DEFAULT_MODEL_IDS.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let group = if target == "dsh" { Some(config::DSH_GROUP) } else { None };
    match crate::services::account::ensure_session().await {
        Ok(_) => match crate::services::shop::gateway_models(group).await {
            Ok(models) if !models.is_empty() => (models, false),
            // DSH 卡失败不回退通用清单：返回空列表 + offline，由 UI 提示，防专属隔离被通用模型击穿。
            _ if target == "dsh" => (Vec::new(), true),
            _ => (fallback(), true),
        },
        Err(_) if target == "dsh" => (Vec::new(), true),
        Err(_) => (fallback(), true),
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
    let user_picked_api = base_url.as_deref() == Some("https://api.greenpool.cn");
    let input = write_input(ids.clone(), base_url, &target, data_dir)?;
    let adapter = adapters::find_adapter(&target).ok_or(AppError::TargetNotFound)?;
    let seeds = seeds_for(&ids);
    let mut p = adapter.plan(&input, &seeds, resolve_custom(custom_path, data_dir, &target).as_deref())?;
    // 诚实告知：Node 系客户端选 api 会被强制改用 aigw（IPv6 半通网络下 api 必然超时）
    if user_picked_api && NODE_STACK_TARGETS.contains(&target.as_str()) {
        p.warnings.push(
            "该客户端使用 Node 网络栈（不支持 IPv6→IPv4 回退），已自动改用 aigw.greenpool.cn（同一网关的纯 IPv4 线路）"
                .into(),
        );
    }
    Ok(p)
}

fn apply_locked(
    target: String,
    model_ids: Vec<String>,
    custom_path: Option<String>,
    base_url: Option<String>,
    data_dir: &PathBuf,
) -> AppResult<ApplyResult> {
    let ids = resolve_ids(model_ids);
    let input = write_input(ids.clone(), base_url, &target, data_dir)?;
    let adapter = adapters::find_adapter(&target).ok_or(AppError::TargetNotFound)?;
    let seeds = seeds_for(&ids);
    let custom = resolve_custom(custom_path.clone(), data_dir, &target);
    let result = adapter.apply(&input, &seeds, custom.as_deref())?;

    // 用户手动指定过路径 → 持久化，detect/后续操作都指向它
    if let Some(p) = custom {
        save_override(data_dir, &target, &p.display().to_string())?;
    }

    let mut records = load_records(data_dir)?;

    // 托管态再次导入不产生新备份（adapter 跳过）→ 继承「最近一次且内容干净」的还原点。
    // 只认「备份可读 + 内容未被 GP 托管」的记录：旧版本链式备份（污染快照）不能作为还原点，
    // 因此逐条向前找，直到找到真正的「配置前原始快照」。
    let mut backup_path = result.backup_path.clone();
    if backup_path.is_none() {
        let adapter_ref = adapters::find_adapter(&target);
        backup_path = records
            .iter()
            .rev()
            .filter(|r| r.target == target && r.status != "rolled_back")
            .filter_map(|r| r.backup_path.clone())
            .find(|bp| {
                let p = PathBuf::from(bp);
                let readable = if p.exists() { Some(p) } else { find_mirror(&target, &p, data_dir) };
                match (readable, adapter_ref.as_ref()) {
                    (Some(path), Some(a)) => fs_atomic::read_string(&path)
                        .map(|c| !a.is_managed_content(&c))
                        .unwrap_or(false),
                    _ => false,
                }
            });
    }

    // 备份镜像到 GP 数据目录（按 target 分目录）：同目录 .bak 可能被客户端清理；分目录防同名覆盖。
    if let Some(bp) = &backup_path {
        mirror_backup(&target, Path::new(bp), data_dir);
    }
    for eb in &result.extra_backups {
        mirror_backup(&target, Path::new(&eb.backup), data_dir);
    }

    let record = InstallRecord {
        id: uuid::Uuid::new_v4().to_string(),
        target,
        target_path: Some(result.path.clone()),
        created_at: chrono::Local::now().to_rfc3339(),
        status: result.status.clone(),
        backup_path,
        extra_backups: result.extra_backups.clone(),
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
        apply_locked(target.clone(), model_ids.clone(), custom_path, base_url.clone(), data_dir)?
    };

    if verify && result.verify_ok {
        // 抽样验证最多 3 个模型：只验第一个会漏掉「部分模型在本分组无渠道」的情况
        let models: Vec<String> = resolve_ids(model_ids).into_iter().take(3).collect();
        if let Ok(key) = api_key_for(&target) {
            // 连通自测与实际写入保持一致（Node 系客户端会被 normalize 强制到 aigw；隧道中走本地转发）
            let client = crate::newapi::NewApiClient::new(&normalize_base_url(base_url, &target, data_dir)?);
            for model in models {
                match client.verify_key(&key, &model).await {
                    Ok(Ok(())) => continue,
                    Ok(Err(reason)) => {
                        result.status = "connectivity_failed".into();
                        result.message = format!("已写入，但模型 {} 连接未通过：{}", model, friendly_verify_error(&reason));
                        break;
                    }
                    Err(_) => {
                        result.status = "connectivity_failed".into();
                        result.message = "已写入，但无法完成连接自测（可能离线）".into();
                        break;
                    }
                }
            }
        }
    }
    Ok(result)
}

pub fn rollback(record_id: String, force: bool, data_dir: &PathBuf) -> AppResult<()> {
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

    // 同目录备份可能已被客户端应用清理 → 回退到 GP 数据目录里的镜像副本（按 target 分目录，兼容旧扁平布局）。
    let backup_path = PathBuf::from(&backup);
    let backup_path = if backup_path.exists() {
        backup_path
    } else if let Some(mirror) = find_mirror(&rec.target, &backup_path, data_dir) {
        mirror
    } else {
        return Err(AppError::RollbackFailed(format!(
            "备份文件已丢失（可能被客户端应用清理）：{}", backup
        )));
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
    // 用 adapter 的精确托管判定（与 detect 同口径）——历史污染快照拒绝回退，合法原始备份照常放行。
    // force=true：用户二次确认后强制还原（用于确认自己就是要回退到该快照）。
    let backup_content = fs_atomic::read_string(&backup_path)?;
    if !force && adapter.is_managed_content(&backup_content) {
        return Err(AppError::RollbackBlocked);
    }

    adapter.rollback(&backup_path, &target_path)?;

    // 附加文件（如 DSH .credentials.yaml）一并还原：保证「还原 = 完全回到配置前」，密钥不残留。
    for eb in &rec.extra_backups {
        let bp = PathBuf::from(&eb.backup);
        let bp = if bp.exists() {
            bp
        } else if let Some(m) = find_mirror(&rec.target, &bp, data_dir) {
            m
        } else {
            return Err(AppError::RollbackFailed(format!("附加备份已丢失：{}", eb.backup)));
        };
        let content = fs_atomic::read_string(&bp)?;
        fs_atomic::atomic_write(Path::new(&eb.target), &content)?;
    }

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

/// 备份镜像根目录：<data_dir>/backups/。
fn backup_mirrors_dir(data_dir: &PathBuf) -> PathBuf {
    data_dir.join("backups")
}

/// 把同目录备份复制一份到 GP 数据目录（尽力而为，失败不阻断导入）。
/// 按 target 分目录：WorkBuddy 与 CodeBuddy 都用 models.json，扁平存放会同名覆盖 / 跨目标串台。
fn mirror_backup(target: &str, backup: &Path, data_dir: &PathBuf) {
    let Some(name) = backup.file_name() else { return };
    let dir = backup_mirrors_dir(data_dir).join(target);
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let _ = std::fs::copy(backup, dir.join(name));
}

/// 查找镜像副本：新布局 `<backups>/<target>/<name>`，并兼容旧版扁平布局 `<backups>/<name>`。
fn find_mirror(target: &str, backup: &Path, data_dir: &PathBuf) -> Option<PathBuf> {
    let name = backup.file_name()?;
    let base = backup_mirrors_dir(data_dir);
    let scoped = base.join(target).join(name);
    if scoped.exists() {
        return Some(scoped);
    }
    let legacy = base.join(name);
    legacy.exists().then_some(legacy)
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
    // 容错：外部工具（如 PowerShell 5.x Out-File）改写后可能带 UTF-8 BOM，serde_json 不接受，剥掉再解析。
    let raw = raw.trim_start_matches('\u{FEFF}');
    serde_json::from_str(raw).map_err(|e| {
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

/// 隧道 base_url 改写（产品方案 B）：把「已安装工具配置里 GP 写入的 base_url」在官方域名 ↔ 隧道地址之间互换。
/// 只改 has_managed_entry（GP 托管）的工具；文本级替换，/v1、/v1/chat/completions 等后缀自动保留。
/// to_tunnel=true：官方 → 隧道；false：隧道 → 官方（统一回 aigw，纯 IPv4 更稳的直连线路）。
pub fn rewrite_base_urls(data_dir: &Path, to_tunnel: bool) -> AppResult<usize> {
    let overrides = load_overrides(&data_dir.to_path_buf());
    let mut count = 0usize;
    for a in adapters::all_adapters() {
        if let Ok(det) = a.detect(override_of(&overrides, a.id()).as_deref()) {
            if det.detected && det.has_managed_entry {
                if let Some(p) = &det.path {
                    match rewrite_base_url_file(Path::new(p), to_tunnel) {
                        Ok(true) => count += 1,
                        Ok(false) => {}
                        Err(e) => log::warn!("改写 base_url 失败 {}: {e}", p),
                    }
                }
            }
        }
    }
    Ok(count)
}

fn rewrite_base_url_file(path: &Path, to_tunnel: bool) -> AppResult<bool> {
    let raw = fs_atomic::read_string(path)?;
    let (froms, to): (&[&str], &str) = if to_tunnel {
        (
            &[config::DEFAULT_BASE_URL, config::PROBE_BASE_URL],
            config::TUNNEL_BASE_URL,
        )
    } else {
        (&[config::TUNNEL_BASE_URL], config::PROBE_BASE_URL)
    };
    let mut out = raw.clone();
    let mut changed = false;
    for f in froms {
        if out.contains(f) {
            out = out.replace(f, to);
            changed = true;
        }
    }
    if changed {
        fs_atomic::atomic_write(path, &out)?;
    }
    Ok(changed)
}
