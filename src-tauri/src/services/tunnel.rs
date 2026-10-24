//! 隧道子系统（v5 冻结规范实现）。
//!
//! 核心契约：
//!   - 唯一 TunnelManager（单 Mutex 状态 + 唯一 ssh 子进程 + 唯一后台任务）
//!   - 六态状态机：stopped / connecting / connected / degraded / reconnecting / error
//!   - generation 双检查点：发起 ssh 创建前 + 登记 Child 前各校验一次，防旧任务覆盖新状态
//!   - 三层探活：进程存活 / 本地监听 / API（三态 ok/fail/unchecked），结果附 generation
//!   - 故障归因：HTTP 超时/5xx = 应用层故障（degraded，不重启 SSH）；ssh 死亡/转发失效 = 重建
//!   - 安全：私钥 keyring、host key pin（StrictHostKeyChecking=yes + 预置 known_hosts）

use std::io::{BufRead, BufReader};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{
    engine::general_purpose::{STANDARD, STANDARD_NO_PAD},
    Engine as _,
};
use serde::Serialize;
use sha2::{Digest, Sha256};
use tauri::{AppHandle, Emitter, Manager};

use crate::config;
use crate::credential;
use crate::error::{AppError, AppResult};
use crate::fs_atomic;
use crate::services::secrets::{self, Forward, Secrets};

/// 编译期固定的服务端 host key 指纹。直链加速未启用（产品决策）：占位值，
/// 启用时填回真实指纹（2026-10-21 ssh-keyscan 获取：SHA256:Jv68QKIVzMh8Su8hiDMLhBE0tfC0C2EM5l3pYeWBeUc）。
/// 注意：GPHub 旧版文档的 `SHA256:ALurh41A/...` 是客户端私钥指纹，禁用。
const EXPECTED_HOST_KEY_FINGERPRINT: &str = "SHA256:Jv68QKIVzMh8Su8hiDMLhBE0tfC0C2EM5l3pYeWBeUc";

const PROCESS_CHECK_INTERVAL: Duration = Duration::from_secs(3);
const LISTEN_CHECK_INTERVAL: Duration = Duration::from_secs(5);

/// GUI 进程启动控制台子进程（netstat/ssh）的 Windows 标志：仅 CREATE_NO_WINDOW（不弹控制台窗口）。
/// 错误对话框的抑制由**父进程** SetErrorMode 负责（见 lib.rs suppress_child_error_dialogs），
/// 不能用 CREATE_DEFAULT_ERROR_MODE——那个 flag 语义是「子进程用系统默认错误模式（显示对话框）」，
/// 与「抑制弹框」目标相反。
#[cfg(windows)]
use std::os::windows::process::CommandExt;
#[cfg(windows)]
const CHILD_PROC_FLAGS: u32 = 0x0800_0000; // CREATE_NO_WINDOW

/// netstat 连续失败计数与熔断标志：连续 3 次启动失败后，本轮隧道运行期内降级为纯 TCP 探测。
static NETSTAT_FAILS: AtomicU32 = AtomicU32::new(0);
static NETSTAT_DISABLED: AtomicBool = AtomicBool::new(false);
const API_CHECK_INTERVAL: Duration = Duration::from_secs(15);
const API_TIMEOUT_SECS: u64 = 2;
const LISTEN_TIMEOUT_MS: u64 = 350;
const BACKOFF_START_S: u64 = 1;
const BACKOFF_CAP_S: u64 = 300;
const RECONNECT_ERROR_THRESHOLD: u32 = 20;
const GREEN_REQUIRED: u32 = 3;

// ---------------------------------------------------------------------------
// 类型与事件载荷
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TunnelPhase {
    Stopped,
    Connecting,
    Connected,
    Degraded,
    Reconnecting,
    Error,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ApiState {
    Ok,
    Fail,
    Unchecked,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LogLine {
    pub line: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HealthPoint {
    pub endpoint: String,
    pub listen_ok: bool,
    pub api_state: ApiState,
    pub last_error: Option<String>,
    /// 探活迟滞计数（内部用，不序列化）：连续 2 次失败才判定 listen 失效，
    /// 防单次探活抖动（netstat 偶发失败/系统繁忙）导致状态跳「重连中」、甚至误重连。
    #[serde(skip)]
    pub listen_streak: u32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TunnelStatus {
    pub phase: TunnelPhase,
    pub message: String,
    pub connected_at: Option<i64>,
    pub last_connect_ms: Option<u64>,
    pub has_key: bool,
    pub reconnect_count: u32,
    pub ssh_alive: bool,
    pub health: Vec<HealthPoint>,
    pub forwards: Vec<Forward>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProbeResult {
    pub ok: bool,
    pub listen_ms: Option<u64>,
    pub api_ms: Option<u64>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportKeyResult {
    pub ok: bool,
    pub key_fingerprint: Option<String>,
}

// ---------------------------------------------------------------------------
// 运行时状态（进程内单例）
// ---------------------------------------------------------------------------

struct ManagerState {
    phase: TunnelPhase,
    message: String,
    generation: u64,
    child: Option<Child>,
    tmp_dir: Option<tempfile::TempDir>,
    connected_at: Option<i64>,
    last_connect_ms: Option<u64>,
    connect_started: Option<Instant>,
    reconnect_count: u32,
    backoff_s: u64,
    consecutive_green: u32,
    consecutive_fail: u32,
    ssh_alive: bool,
    health: Vec<HealthPoint>,
    forwards: Vec<Forward>,
    backoff_deadline: Option<Instant>,
    /// 运行期端口冲突：本地端口被「非本隧道 ssh」的进程占用。为 true 时禁止重建（§5.5 归属判定）。
    port_conflict: bool,
    /// 致命错误（HostKey 不匹配/私钥缺失等配置安全错误）：暂停自动重试，等待用户处理（P0-3）。
    fatal: bool,
}

impl Default for ManagerState {
    fn default() -> Self {
        let s = Secrets::default();
        let health = s
            .forwards
            .iter()
            .filter(|f| f.enabled)
            .map(|f| HealthPoint {
                endpoint: f.id.clone(),
                listen_ok: false,
                api_state: ApiState::Unchecked,
                last_error: None,
                listen_streak: 0,
            })
            .collect();
        Self {
            phase: TunnelPhase::Stopped,
            message: String::new(),
            generation: 0,
            child: None,
            tmp_dir: None,
            connected_at: None,
            last_connect_ms: None,
            connect_started: None,
            reconnect_count: 0,
            backoff_s: BACKOFF_START_S,
            consecutive_green: 0,
            consecutive_fail: 0,
            ssh_alive: false,
            health,
            forwards: s.forwards,
            backoff_deadline: None,
            port_conflict: false,
            fatal: false,
        }
    }
}

static STATE: Mutex<Option<ManagerState>> = Mutex::new(None);
static BG_RUNNING: AtomicBool = AtomicBool::new(false);

fn lock() -> std::sync::MutexGuard<'static, Option<ManagerState>> {
    STATE.lock().unwrap_or_else(|p| p.into_inner())
}

fn now_ts() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

// ---------------------------------------------------------------------------
// 状态快照
// ---------------------------------------------------------------------------

pub fn status(data_dir: &Path) -> TunnelStatus {
    // 配置加载失败要显式可见，不静默回退默认端点（审计 P1）
    let (s, load_err) = match secrets::load(data_dir) {
        Ok(s) => (s, None),
        Err(e) => (Secrets::default(), Some(e.to_string())),
    };
    let has_key = credential::get(config::CREDENTIAL_SSH_PRIVATE_KEY)
        .unwrap_or(None)
        .is_some();
    let (
        phase,
        message,
        connected_at,
        last_connect_ms,
        reconnect_count,
        ssh_alive,
        health,
        forwards,
    ) = {
        let st = lock();
        match st.as_ref() {
            Some(m) => (
                m.phase,
                m.message.clone(),
                m.connected_at,
                m.last_connect_ms,
                m.reconnect_count,
                m.ssh_alive,
                m.health.clone(),
                m.forwards.clone(),
            ),
            None => (
                TunnelPhase::Stopped,
                String::new(),
                None,
                None,
                0,
                false,
                s.forwards
                    .iter()
                    .filter(|f| f.enabled)
                    .map(|f| HealthPoint {
                        endpoint: f.id.clone(),
                        listen_ok: false,
                        api_state: ApiState::Unchecked,
                        last_error: None,
                        listen_streak: 0,
                    })
                    .collect(),
                s.forwards,
            ),
        }
    };
    let mut st = TunnelStatus {
        phase,
        message,
        connected_at,
        last_connect_ms,
        has_key,
        reconnect_count,
        ssh_alive,
        health,
        forwards,
    };
    if let Some(err) = load_err {
        st.phase = TunnelPhase::Error;
        st.message = format!("加密资源加载失败：{err}");
    }
    st
}

fn emit_status(app: &AppHandle) {
    let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
    let _ = app.emit(config::TUNNEL_EVENT_STATUS, status(&data_dir));
}

// ---------------------------------------------------------------------------
// host key pin
// ---------------------------------------------------------------------------

fn decode_host_key(b64: &str) -> AppResult<Vec<u8>> {
    STANDARD
        .decode(b64.trim())
        .map_err(|e| AppError::Tunnel(format!("host key 不是合法 base64：{e}")))
}

pub fn fingerprint(raw_key: &[u8]) -> String {
    let digest = Sha256::digest(raw_key);
    // OpenSSH 指纹用无 padding 的 base64（ssh-keygen -lf 同款），故用 STANDARD_NO_PAD。
    format!("SHA256:{}", STANDARD_NO_PAD.encode(digest.as_slice()))
}

fn verified_known_hosts(s: &Secrets) -> AppResult<String> {
    if s.host_key_b64.trim().is_empty() {
        return Err(AppError::HostKeyMissing);
    }
    let fp = fingerprint(&decode_host_key(&s.host_key_b64)?);
    if fp != EXPECTED_HOST_KEY_FINGERPRINT {
        return Err(AppError::HostKeyMismatch);
    }
    Ok(format!(
        "[{}]:{} {} {}",
        s.ssh_host, s.ssh_port, s.host_key_type, s.host_key_b64
    ))
}

// ---------------------------------------------------------------------------
// ssh 子进程
// ---------------------------------------------------------------------------

/// 解析 ssh 可执行文件：优先系统 OpenSSH 固定路径（防 PATH 劫持，P1），fallback PATH。
fn ssh_command() -> Command {
    #[cfg(windows)]
    {
        const SYS_SSH: &str = r"C:\Windows\System32\OpenSSH\ssh.exe";
        if std::path::Path::new(SYS_SSH).exists() {
            return Command::new(SYS_SSH);
        }
    }
    Command::new("ssh")
}

/// 解析 ssh-keygen 可执行文件（同 ssh_command 的可信路径策略）。
fn ssh_keygen_command() -> Command {
    #[cfg(windows)]
    {
        const SYS_KEYGEN: &str = r"C:\Windows\System32\OpenSSH\ssh-keygen.exe";
        if std::path::Path::new(SYS_KEYGEN).exists() {
            return Command::new(SYS_KEYGEN);
        }
    }
    Command::new("ssh-keygen")
}

fn build_ssh_cmd(s: &Secrets, key_path: &Path, kh_path: &Path) -> Command {
    let mut cmd = ssh_command();
    #[cfg(windows)]
    cmd.creation_flags(CHILD_PROC_FLAGS);
    cmd.arg("-N")
        .arg("-o")
        .arg("IdentitiesOnly=yes")
        .arg("-o")
        .arg("StrictHostKeyChecking=yes")
        .arg("-o")
        .arg(format!("UserKnownHostsFile={}", kh_path.display()))
        .arg("-o")
        .arg("ExitOnForwardFailure=yes")
        .arg("-o")
        .arg("ServerAliveInterval=5")
        .arg("-o")
        .arg("ServerAliveCountMax=2")
        .arg("-o")
        .arg("ConnectTimeout=8")
        .arg("-o")
        .arg("BatchMode=yes")
        .arg("-p")
        .arg(s.ssh_port.to_string())
        .arg("-i")
        .arg(key_path);
    for f in &s.forwards {
        if f.enabled {
            cmd.arg("-L")
                .arg(format!("127.0.0.1:{}:127.0.0.1:{}", f.local_port, f.target_port));
        }
    }
    cmd.arg(format!("{}@{}", s.ssh_user, s.ssh_host))
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::piped());
    cmd
}

/// 准备临时文件（私钥 + known_hosts）并 spawn ssh。返回 (child, tmpdir)。
fn spawn_ssh(s: &Secrets, key_pem: &str, kh_line: &str) -> AppResult<(Child, tempfile::TempDir)> {
    let tmp = tempfile::tempdir()?;
    let key_path = tmp.path().join("gp_tunnel_key");
    // OpenSSH 对私钥文件极严：①末尾必须有换行（否则 "invalid format"）；②权限只允许当前用户（否则 "bad permissions"）。
    std::fs::write(&key_path, format!("{}\n", key_pem.trim_end()))?;
    fs_atomic::secure_permissions(&key_path)?;
    let kh_path = tmp.path().join("known_hosts");
    std::fs::write(&kh_path, format!("{kh_line}\n"))?;
    fs_atomic::secure_permissions(&kh_path)?;

    let child = build_ssh_cmd(s, &key_path, &kh_path)
        .spawn()
        .map_err(|e| AppError::Tunnel(format!("无法启动 ssh：{e}")))?;
    Ok((child, tmp))
}

fn port_listening(port: u16) -> bool {
    TcpStream::connect_timeout(
        &format!("127.0.0.1:{port}").parse().unwrap(),
        Duration::from_millis(LISTEN_TIMEOUT_MS),
    )
    .is_ok()
}

/// 用 netstat 查本机 IPv4 LISTEN 端口的 owning PID（区分「谁在监听」，而非仅「能否连」）。
/// 用于 §5.5 端口归属判定：把本地端口归属与 ssh 子进程 pid 比对，避免误判/误杀其他进程。
fn listening_ports() -> std::collections::HashMap<u16, u32> {
    let mut map = std::collections::HashMap::new();
    // 熔断：netstat 连续失败后本轮运行期改用纯 TCP 探测（check_listen 的 degraded 分支），
    // 避免探活高频重试 netstat 失败 → Windows 系统错误框无限弹出。
    if NETSTAT_DISABLED.load(Ordering::Relaxed) {
        return map;
    }
    let mut cmd = Command::new("netstat");
    cmd.args(["-ano", "-p", "tcp"]);
    #[cfg(windows)]
    cmd.creation_flags(CHILD_PROC_FLAGS);
    let out = match cmd.output() {
        Ok(o) if o.status.success() => o,
        _ => {
            let n = NETSTAT_FAILS.fetch_add(1, Ordering::Relaxed) + 1;
            if n >= 3 {
                NETSTAT_DISABLED.store(true, Ordering::Relaxed);
                log::warn!("netstat 连续 {} 次失败，端口归属探测降级为纯 TCP 探测", n);
            }
            return map;
        }
    };
    NETSTAT_FAILS.store(0, Ordering::Relaxed);
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        if !line.to_uppercase().contains("LISTEN") {
            continue;
        }
        let fields: Vec<&str> = line.split_whitespace().collect();
        // netstat 列：Proto LocalAddress ForeignAddress State PID
        let Some(addr) = fields.get(1) else { continue };
        if !addr.starts_with("127.0.0.1") && !addr.starts_with("0.0.0.0") {
            continue;
        }
        let Some(pid) = fields.last().and_then(|s| s.parse::<u32>().ok()) else {
            continue;
        };
        if let Some(port_str) = addr.rsplit(':').next() {
            if let Ok(port) = port_str.parse::<u16>() {
                map.entry(port).or_insert(pid);
            }
        }
    }
    map
}

/// 判断 PID 是否为 ssh 进程（tasklist 过滤；非 Windows 恒 false，不误清理）。
#[cfg(windows)]
fn is_ssh_process(pid: u32) -> bool {
    Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/FO", "CSV", "/NH"])
        .creation_flags(CHILD_PROC_FLAGS)
        .output()
        .map(|o| String::from_utf8_lossy(&o.stdout).to_lowercase().contains("ssh.exe"))
        .unwrap_or(false)
}

#[cfg(not(windows))]
fn is_ssh_process(_pid: u32) -> bool {
    false
}

/// P0 G2 修复：清理「占用隧道本地端口、但不是本进程 Child」的孤儿 ssh。
///
/// 上一实例被强杀/异常退出时，ssh.exe 会脱离父子链成为孤儿并继续占用本地转发端口：
///   - watchdog 观察不到它（Child 不在本进程状态里）→ ssh 挂了没人重启；
///   - stop 杀不掉它 → 端口 3000 仍被占 → 新 ssh 撞 `ExitOnForwardFailure=yes` 直接失败。
///
/// 只杀「端口 owner 是 ssh.exe」的进程（非 ssh 的占用留给端口冲突报错，不误杀）。
/// 返回清理的进程数。
fn kill_orphan_tunnel_ssh(ports: &[u16]) -> usize {
    let current = { lock().as_ref().and_then(|s| s.child.as_ref().map(|c| c.id())) };
    let owners = listening_ports();
    let mut killed = 0usize;
    for port in ports {
        let Some(pid) = owners.get(port).copied() else { continue };
        if Some(pid) == current {
            continue; // 本进程自己的 child，不动
        }
        if !is_ssh_process(pid) {
            continue; // 非 ssh 占用：交给端口冲突报错，不误杀
        }
        #[cfg(windows)]
        let ok = Command::new("taskkill")
            .args(["/F", "/PID", &pid.to_string()])
            .creation_flags(CHILD_PROC_FLAGS)
            .output()
            .map(|o| o.status.success())
            .unwrap_or(false);
        #[cfg(not(windows))]
        let ok = false;
        if ok {
            killed += 1;
            log::warn!("已清理孤儿隧道 ssh（pid={pid}，端口 {port}）");
        }
    }
    killed
}

/// ssh stderr 落盘 + 5MB 轮转（§5.8：日志不无限增长）。emit 到前端另由调用方处理。
fn append_log(data_dir: &Path, line: &str) {
    use std::io::Write;
    let path = data_dir.join("tunnel.log");
    const MAX_LOG_BYTES: u64 = 5 * 1024 * 1024;
    if let Ok(meta) = std::fs::metadata(&path) {
        if meta.len() >= MAX_LOG_BYTES {
            let _ = std::fs::rename(&path, data_dir.join("tunnel.log.1"));
        }
    }
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{line}");
    }
}

// ---------------------------------------------------------------------------
// 启动 / 停止 / 重启（同步，stop 供退出钩子直接调用）
// ---------------------------------------------------------------------------

pub fn start(app: &AppHandle, data_dir: &Path) -> AppResult<()> {
    // 后端总开关（V6）：前端按钮禁用之外的硬性拦截，防绕过 UI 直接调用命令。
    if !config::TURBO_ENABLED {
        return Err(AppError::Tunnel("直链加速功能尚未启用".into()));
    }
    // 重置 netstat 熔断状态（每次启动隧道重新给予归属探测机会）。
    NETSTAT_FAILS.store(0, Ordering::Relaxed);
    NETSTAT_DISABLED.store(false, Ordering::Relaxed);
    // 端口冲突前置检测（锁外，阻塞操作不持锁）；配置加载失败直接报错，不静默回退
    let s = secrets::load(data_dir)?;
    let ports: Vec<u16> = s.forwards.iter().filter(|f| f.enabled).map(|f| f.local_port).collect();
    // P0 G2 修复：先清理上一实例遗留的孤儿 ssh（强杀/崩溃时 ssh 脱离父子链占端口），
    // 否则下面的端口冲突检测会直接拒绝启动，且 stop 也杀不掉它。
    if kill_orphan_tunnel_ssh(&ports) > 0 {
        std::thread::sleep(Duration::from_millis(300)); // 等 socket 释放
    }
    let conflicts: Vec<u16> = ports.iter().copied().filter(|p| port_listening(*p)).collect();
    if !conflicts.is_empty() {
        return Err(AppError::Tunnel(format!("端口冲突：{:?} 已被占用", conflicts)));
    }

    // P0 G2 修复：覆盖运行时状态前，先强制终止本进程残留的旧 child
    // （Rust 的 Child Drop 不会 kill 进程，旧 ssh 会泄漏成孤儿）。
    {
        let (stale_child, stale_tmp) = {
            let mut st = lock();
            match st.as_mut() {
                Some(state) => (state.child.take(), state.tmp_dir.take()),
                None => (None, None),
            }
        };
        if let Some(mut c) = stale_child {
            let _ = c.kill();
            let _ = c.wait();
        }
        drop(stale_tmp); // 一并清理旧临时私钥目录
    }

    // 原子认领启动权 + 递增 generation（P0-2：用户主动重试必须让旧轮次/旧后台任务失效）
    let gen = {
        let mut st = lock();
        let state = st.get_or_insert_with(ManagerState::default);
        match state.phase {
            TunnelPhase::Connecting | TunnelPhase::Connected | TunnelPhase::Degraded => return Ok(()), // 运行中，幂等
            TunnelPhase::Stopped | TunnelPhase::Reconnecting | TunnelPhase::Error => {}
        }
        state.generation = state.generation.wrapping_add(1); // 关键：递增，旧 generation 的 reconcile/spawn 全部失效
        state.phase = TunnelPhase::Connecting;
        state.message = "正在建立隧道…".into();
        state.backoff_s = BACKOFF_START_S;
        state.reconnect_count = 0;
        state.consecutive_fail = 0;
        state.backoff_deadline = None;
        state.port_conflict = false;
        state.fatal = false;
        state.generation
    };

    ensure_background(app.clone(), data_dir.to_path_buf());
    match spawn_and_register(app, data_dir, gen) {
        Ok(()) => Ok(()),
        Err(e) => {
            handle_spawn_err(gen, app, &e);
            Err(e)
        }
    }
}

pub fn stop(app: &AppHandle) -> AppResult<()> {
    let (child, tmp) = {
        let mut st = lock();
        let Some(state) = st.as_mut() else {
            return Ok(());
        };
        state.generation = state.generation.wrapping_add(1); // 旧后台任务失效
        state.phase = TunnelPhase::Stopped;
        state.message = String::new();
        state.connected_at = None;
        state.last_connect_ms = None;
        state.connect_started = None;
        state.ssh_alive = false;
        state.reconnect_count = 0;
        state.backoff_s = BACKOFF_START_S;
        state.consecutive_green = 0;
        state.consecutive_fail = 0;
        state.backoff_deadline = None;
        state.port_conflict = false;
        state.fatal = false;
        for h in &mut state.health {
            h.listen_ok = false;
            h.api_state = ApiState::Unchecked;
            h.last_error = None;
            h.listen_streak = 0;
        }
        (state.child.take(), state.tmp_dir.take())
    };

    // 锁外：杀进程（3s 超时兜底，§5.4）+ 清理临时目录（回读校验，§4.5）
    if let Some(mut child) = child {
        let _ = child.kill();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        log::warn!("ssh 进程 3s 内未退出，已强制超时");
                        break;
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
                Err(_) => break,
            }
        }
    }
    if let Some(tmp) = tmp {
        let path = tmp.path().to_path_buf();
        drop(tmp); // TempDir drop → 删除目录
        if path.exists() {
            log::warn!("临时私钥目录删除失败（残留）：{}", path.display());
        }
    }

    // P0 G2 兜底：child 不在（孤儿遗留）时，清理端口上残留的 ssh.exe，
    // 防「stop 杀不掉、端口仍被占、下次 start 直接失败」。
    {
        let data_dir = app.state::<crate::state::AppState>().data_dir.clone();
        if let Ok(s) = secrets::load(&data_dir) {
            let ports: Vec<u16> = s.forwards.iter().filter(|f| f.enabled).map(|f| f.local_port).collect();
            let _ = kill_orphan_tunnel_ssh(&ports);
        }
    }

    emit_status(app);
    Ok(())
}

pub fn restart(app: &AppHandle, data_dir: &Path) -> AppResult<()> {
    stop(app)?;
    start(app, data_dir)
}

// ---------------------------------------------------------------------------
// 后台任务（唯一）
// ---------------------------------------------------------------------------

fn ensure_background(app: AppHandle, data_dir: PathBuf) {
    if BG_RUNNING.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        background_loop(app, data_dir).await;
    });
}

async fn background_loop(app: AppHandle, data_dir: PathBuf) {
    let mut last_process = Instant::now();
    let mut last_listen = Instant::now();
    let mut last_api = Instant::now();
    loop {
        tokio::time::sleep(Duration::from_millis(500)).await;

        let (gen, phase) = {
            let st = lock();
            match st.as_ref() {
                Some(s) => (s.generation, s.phase),
                None => continue,
            }
        };
        if phase == TunnelPhase::Stopped {
            continue;
        }

        if last_process.elapsed() >= PROCESS_CHECK_INTERVAL {
            last_process = Instant::now();
            check_process(gen, &app);
        }
        if last_listen.elapsed() >= LISTEN_CHECK_INTERVAL {
            last_listen = Instant::now();
            check_listen(gen, &app, &data_dir);
        }
        if last_api.elapsed() >= API_CHECK_INTERVAL {
            last_api = Instant::now();
            check_api(gen, &app, &data_dir).await;
        }

        reconcile(gen, &app, &data_dir);
    }
}

/// 进程巡检：检测 ssh 子进程退出（只记录，phase 由 reconcile 合成）。
/// try_wait 三态：StillRunning / Exited / WaitError，不再把错误吞成「仍存在」。
fn check_process(gen: u64, app: &AppHandle) {
    let mut exited: Option<(bool, Option<i32>)> = None;
    let mut tmp_to_clean: Option<tempfile::TempDir> = None;

    {
        let mut st = lock();
        let Some(state) = st.as_mut() else { return };
        if state.generation != gen {
            return;
        }
        match state.child.as_mut().map(|c| c.try_wait()) {
            Some(Ok(Some(status))) => {
                // 正常退出：回收 child + 立即取出临时目录（P1：ssh 退出后不留私钥残留）
                state.child = None;
                state.ssh_alive = false;
                tmp_to_clean = state.tmp_dir.take();
                exited = Some((status.success(), status.code()));
            }
            Some(Ok(None)) => {
                state.ssh_alive = true; // 仍运行
            }
            Some(Err(_)) => {
                // WaitError：状态未知，保守按失联处理，放弃该 child 管理以允许重连（避免 !child_exists 卡死）
                log::warn!("ssh 子进程状态查询失败，按失联处理");
                state.child = None;
                state.ssh_alive = false;
                tmp_to_clean = state.tmp_dir.take();
            }
            None => {
                state.ssh_alive = false; // 无 child
            }
        }
    }

    // 锁外清理临时目录（回读校验）
    if let Some(tmp) = tmp_to_clean {
        let path = tmp.path().to_path_buf();
        drop(tmp);
        if path.exists() {
            log::warn!("临时私钥目录删除失败（残留）：{}", path.display());
        }
    }

    if let Some((success, code)) = exited {
        let mut st = lock();
        let Some(state) = st.as_mut() else { return };
        if state.generation != gen {
            return;
        }
        state.connected_at = None;
        state.consecutive_fail += 1;
        state.message = if success {
            "隧道已退出".into()
        } else {
            format!("隧道断开（exit {}）", code.unwrap_or(-1))
        };
        drop(st);
        emit_status(app);
    }
}

/// 监听探测：更新各端点 listenOk，并在首次全部就绪时记录建连耗时。
fn check_listen(gen: u64, app: &AppHandle, data_dir: &Path) {
    let s = match secrets::load(data_dir) {
        Ok(s) => s,
        Err(_) => return, // 配置损坏：不探活，由 status 暴露错误
    };
    // 锁外执行 netstat（阻塞操作不持锁），读 ssh pid 快照
    let owners = listening_ports();
    let ssh_pid = {
        let st = lock();
        st.as_ref().and_then(|s| s.child.as_ref().map(|c| c.id()))
    };
    let mut changed = false;
    let mut all_listen = true;
    let mut any_conflict = false;

    {
        let mut st = lock();
        let Some(state) = st.as_mut() else { return };
        if state.generation != gen {
            return;
        }
        // netstat 熔断后降级：纯 TCP 探测（不校验归属 PID，netstat 不可用时的合理折中）。
        let degraded = NETSTAT_DISABLED.load(Ordering::Relaxed);
        for f in s.forwards.iter().filter(|f| f.enabled) {
            let ok = if degraded {
                port_listening(f.local_port)
            } else {
                // 归属判定（§5.5）：本地端口 owning PID 等于本隧道 ssh pid → 转发可用。
                // netstat 偶发无数据（空/解析失败）时用 TCP 探测兜底，避免误判掉线触发重连。
                match owners.get(&f.local_port).copied() {
                    Some(pid) => {
                        let mine = ssh_pid == Some(pid);
                        // 端口被「非本隧道 ssh」的进程占用 → 冲突（禁止重建，不误杀）
                        if !mine {
                            any_conflict = true;
                        }
                        mine
                    }
                    None => port_listening(f.local_port),
                }
            };
            if !ok {
                all_listen = false;
            }
            if let Some(h) = state.health.iter_mut().find(|h| h.endpoint == f.id) {
                if !ok && h.listen_ok {
                    // 掉线迟滞：连续 2 次失败才判定失效（防单次探活抖动导致状态跳/误重连）。
                    h.listen_streak += 1;
                    if h.listen_streak >= 2 {
                        h.listen_ok = false;
                        h.listen_streak = 0;
                        changed = true;
                    }
                } else {
                    // 恢复立即生效（端口一就绪就亮绿），并清零迟滞计数。
                    if ok && !h.listen_ok {
                        h.listen_ok = true;
                        changed = true;
                    }
                    h.listen_streak = 0;
                }
            }
        }
        if state.port_conflict != any_conflict {
            state.port_conflict = any_conflict;
            changed = true;
        }
        if all_listen && state.last_connect_ms.is_none() {
            if let Some(start) = state.connect_started.take() {
                state.last_connect_ms = Some(start.elapsed().as_millis() as u64);
            }
        }
    }

    if changed {
        emit_status(app);
    }
}

/// API 探活：更新各端点 apiState（三态）。结果附 generation，旧结果丢弃。
async fn check_api(gen: u64, app: &AppHandle, data_dir: &Path) {
    let s = match secrets::load(data_dir) {
        Ok(s) => s,
        Err(_) => return, // 配置损坏：不探活，由 status 暴露错误
    };
    let client = crate::services::http_client(1, API_TIMEOUT_SECS);

    let mut results: Vec<(String, ApiState, Option<String>)> = Vec::new();
    for f in s.forwards.iter().filter(|f| f.enabled) {
        match &f.probe_path {
            None => results.push((f.id.clone(), ApiState::Unchecked, None)),
            Some(path) => {
                let url = format!("http://127.0.0.1:{}{}", f.local_port, path);
                match client.get(&url).send().await {
                    Ok(r) => {
                        let status = r.status().as_u16();
                        if f.probe_success_status.contains(&status) {
                            results.push((f.id.clone(), ApiState::Ok, None));
                        } else {
                            results.push((f.id.clone(), ApiState::Fail, Some(format!("http {status}"))));
                        }
                    }
                    Err(e) => results.push((f.id.clone(), ApiState::Fail, Some(e.to_string()))),
                }
            }
        }
    }

    let mut changed = false;
    {
        let mut st = lock();
        let Some(state) = st.as_mut() else { return };
        if state.generation != gen {
            return; // 旧轮次结果，丢弃
        }
        for (endpoint, api_state, err) in results {
            if let Some(h) = state.health.iter_mut().find(|h| h.endpoint == endpoint) {
                if h.api_state != api_state {
                    h.api_state = api_state;
                    changed = true;
                }
                h.last_error = err;
            }
        }
    }

    if changed {
        emit_status(app);
    }
}

/// 状态合成真值表（纯函数，供单测验证归因正确性）。
/// - ssh 死亡 / 监听失效 → Reconnecting（SSH 层故障，需重建）
/// - API 故障（监听通但 API 挂）→ Degraded（应用层故障，不重建 SSH）
/// - 其余 → Connecting（等待连续全绿）
fn synthesize_phase(ssh_alive: bool, all_listen: bool, any_api_fail: bool) -> TunnelPhase {
    if !ssh_alive || !all_listen {
        TunnelPhase::Reconnecting
    } else if any_api_fail {
        TunnelPhase::Degraded
    } else {
        TunnelPhase::Connecting
    }
}

/// 退避翻倍（封顶 300s）+ 随机抖动（±25%）。
/// 抖动用于打散「服务器重启 / 网络恢复后所有客户端同一秒重连」的同步风暴
/// （批量分发场景下 30 个客户端同时建连会撞 MaxStartups）。
fn next_backoff(s: u64) -> u64 {
    let raw = (s * 2).min(BACKOFF_CAP_S);
    let jitter_max = (raw / 4).max(1);
    // 无 rand 依赖：用时钟纳秒取模作为伪随机抖动。
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos() as u64)
        .unwrap_or(0)
        % jitter_max;
    // 抖动后仍封顶（抖动不应突破 BACKOFF_CAP_S）
    (raw + jitter).min(BACKOFF_CAP_S)
}

/// 致命隧道错误：安全/配置类错误，重试不会自愈，必须显式报错并暂停自动重试（P0-3）。
fn is_fatal_tunnel_err(e: &AppError) -> bool {
    matches!(
        e,
        AppError::HostKeyMissing | AppError::HostKeyMismatch | AppError::SshKeyMissing
    )
}

/// 处理 spawn 失败：致命错误 → 置 error 并暂停重试；可重试错误 → 累计 consecutive_fail（让 error 阈值可触发）。
fn handle_spawn_err(gen: u64, app: &AppHandle, e: &AppError) {
    let mut st = lock();
    let Some(state) = st.as_mut() else { return };
    if state.generation != gen {
        return; // 旧轮次，忽略
    }
    if is_fatal_tunnel_err(e) {
        state.phase = TunnelPhase::Error;
        state.message = e.to_string();
        state.fatal = true; // 完全暂停自动重试，等待用户处理（用户 restart 会清除）
        state.backoff_s = BACKOFF_CAP_S;
        state.backoff_deadline = Some(Instant::now() + Duration::from_secs(BACKOFF_CAP_S));
    } else {
        // 可重试（如 ssh spawn 失败）：累计失败次数，让退避与 error 阈值生效
        state.consecutive_fail += 1;
    }
    drop(st);
    emit_status(app);
}

/// 状态合成（真值表）+ 重连决策。
fn reconcile(gen: u64, app: &AppHandle, data_dir: &Path) {
    let (ssh_alive, health, child_exists, port_conflict) = {
        let st = lock();
        match st.as_ref() {
            Some(s) => (s.ssh_alive, s.health.clone(), s.child.is_some(), s.port_conflict),
            None => return,
        }
    };

    let all_listen = health.iter().all(|h| h.listen_ok);
    let any_api_fail = health.iter().any(|h| h.api_state == ApiState::Fail);
    let all_green = ssh_alive && all_listen && !any_api_fail;

    let mut changed = false;
    let mut need_reconnect = false;
    let mut entered_connected = false;

    {
        let mut st = lock();
        let Some(state) = st.as_mut() else { return };
        if state.generation != gen {
            return;
        }

        if all_green {
            state.consecutive_green += 1;
            state.consecutive_fail = 0;
        } else {
            state.consecutive_green = 0;
        }

        // 一次性定态（P1-5 修复）：先算出最终目标态，再只设一次，不在 Reconnecting↔Error 之间每轮抖动。
        let base = synthesize_phase(ssh_alive, all_listen, any_api_fail);
        let target = if all_green && state.consecutive_green >= GREEN_REQUIRED {
            TunnelPhase::Connected
        } else {
            match base {
                TunnelPhase::Reconnecting if port_conflict => TunnelPhase::Error, // 端口被占：明确报错，禁止重建
                TunnelPhase::Reconnecting if state.consecutive_fail >= RECONNECT_ERROR_THRESHOLD => TunnelPhase::Error, // 达阈值
                other => other,
            }
        };

        if target != state.phase {
            state.phase = target;
            changed = true;
            match target {
                TunnelPhase::Connected => {
                    state.message.clear();
                    state.connected_at = Some(now_ts());
                    state.backoff_s = BACKOFF_START_S;
                    state.consecutive_fail = 0;
                    state.backoff_deadline = None;
                    entered_connected = true;
                }
                TunnelPhase::Error => {
                    state.message = if port_conflict {
                        "端口被其他程序占用，请释放端口后重启".into()
                    } else {
                        "长时间无法连接，后台持续重试中".into()
                    };
                }
                _ => {}
            }
        }

        // 重连判定：ssh 层故障 + 无存活 child + 非端口冲突 + 非致命错误（error 态继续低频退避重试）
        let ssh_layer_broken = !ssh_alive || !all_listen;
        if ssh_layer_broken && !child_exists && !port_conflict && !state.fatal {
            let now = Instant::now();
            match state.backoff_deadline {
                None => {
                    state.backoff_deadline = Some(now + Duration::from_secs(state.backoff_s));
                    need_reconnect = true;
                }
                Some(d) if now >= d => {
                    state.backoff_s = next_backoff(state.backoff_s);
                    state.backoff_deadline = Some(now + Duration::from_secs(state.backoff_s));
                    need_reconnect = true;
                }
                Some(_) => {}
            }
        }
    }

    if changed {
        emit_status(app);
    }
    if entered_connected {
        // 隧道已连接：把已安装工具的 base_url 改写为隧道地址（方案 B，对用户无感）
        let _ = crate::services::install::rewrite_base_urls(data_dir, true);
    }
    if need_reconnect {
        if let Err(e) = spawn_and_register(app, data_dir, gen) {
            handle_spawn_err(gen, app, &e);
        }
    }
}

/// 创建 ssh 并登记（generation 双检查点）。
fn spawn_and_register(app: &AppHandle, data_dir: &Path, gen: u64) -> AppResult<()> {
    let s = secrets::load(data_dir)?;
    let kh_line = verified_known_hosts(&s)?;
    let key_pem = credential::get(config::CREDENTIAL_SSH_PRIVATE_KEY)?.ok_or(AppError::SshKeyMissing)?;

    // 发起前校验 generation
    {
        let st = lock();
        if let Some(state) = st.as_ref() {
            if state.generation != gen {
                return Ok(());
            }
        }
    }

    let (mut child, tmp) = spawn_ssh(&s, &key_pem, &kh_line)?;

    // stderr → tunnel://log（emit 到前端，前端环形 200 条）+ 落盘（5MB 轮转，§5.8）
    if let Some(stderr) = child.stderr.take() {
        let app2 = app.clone();
        let log_dir = data_dir.to_path_buf();
        std::thread::spawn(move || {
            for line in BufReader::new(stderr).lines().filter_map(Result::ok) {
                let _ = app2.emit(config::TUNNEL_EVENT_LOG, LogLine { line: line.clone() });
                append_log(&log_dir, &line);
            }
        });
    }

    // 登记前再校验 generation（双检查点）
    {
        let mut st = lock();
        let Some(state) = st.as_mut() else {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(());
        };
        if state.generation != gen {
            drop(st);
            let _ = child.kill();
            let _ = child.wait();
            return Ok(()); // 旧轮次：清理自己创建的进程，不覆盖
        }
        state.child = Some(child);
        state.tmp_dir = Some(tmp);
        state.ssh_alive = true;
        state.connect_started = Some(Instant::now());
        state.reconnect_count += 1;
        state.phase = TunnelPhase::Connecting;
        state.message = "正在建立隧道…".into();
    }

    // 建连耗时精确测量：ssh 登记后立即轮询端口就绪（不等 5s 巡检周期），就绪即记真实耗时。
    {
        let ports: Vec<u16> = s
            .forwards
            .iter()
            .filter(|f| f.enabled)
            .map(|f| f.local_port)
            .collect();
        std::thread::spawn(move || {
            let deadline = Instant::now() + Duration::from_secs(12);
            while Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(200));
                // generation 快照校验（旧轮次提前退出）
                {
                    let st = lock();
                    match st.as_ref() {
                        Some(state) if state.generation == gen => {}
                        _ => return,
                    }
                }
                // 锁外探测端口（阻塞操作不持锁）
                if ports.iter().all(|p| port_listening(*p)) {
                    let mut st = lock();
                    if let Some(state) = st.as_mut() {
                        if state.generation == gen {
                            if let Some(start) = state.connect_started {
                                state.last_connect_ms = Some(start.elapsed().as_millis() as u64);
                            }
                        }
                    }
                    return;
                }
            }
        });
    }

    emit_status(app);
    Ok(())
}

// ---------------------------------------------------------------------------
// 探测（运行中，指定端点）
// ---------------------------------------------------------------------------

pub async fn probe(data_dir: &Path, endpoint: &str) -> AppResult<ProbeResult> {
    let s = secrets::load(data_dir)?;
    let f = s
        .forwards
        .iter()
        .find(|f| f.id == endpoint)
        .ok_or_else(|| AppError::Other(format!("未知端点：{endpoint}")))?;
    if !f.enabled {
        return Err(AppError::Other(format!("端点 {endpoint} 未启用")));
    }

    let listen_start = Instant::now();
    let listen_ok = port_listening(f.local_port);
    let listen_ms = Some(listen_start.elapsed().as_millis() as u64);

    let (api_ok, api_ms, error) = match &f.probe_path {
        None => (false, None, Some("该端点未配置应用层探活".into())),
        Some(path) => {
            let client = crate::services::http_client(1, API_TIMEOUT_SECS);
            let url = format!("http://127.0.0.1:{}{}", f.local_port, path);
            let start = Instant::now();
            match client.get(&url).send().await {
                Ok(r) => {
                    let ok = f.probe_success_status.contains(&r.status().as_u16());
                    (
                        ok,
                        Some(start.elapsed().as_millis() as u64),
                        if ok {
                            None
                        } else {
                            Some(format!("http {}", r.status().as_u16()))
                        },
                    )
                }
                Err(e) => (false, Some(start.elapsed().as_millis() as u64), Some(e.to_string())),
            }
        }
    };

    Ok(ProbeResult {
        ok: listen_ok && api_ok,
        listen_ms,
        api_ms,
        error,
    })
}

// ---------------------------------------------------------------------------
// 私钥导入（原生文件对话框，私钥不经前端 JS）
// ---------------------------------------------------------------------------

pub fn import_key(app: &AppHandle) -> AppResult<ImportKeyResult> {
    use tauri_plugin_dialog::DialogExt;
    let picked = app.dialog().file().blocking_pick_file();
    let Some(fp) = picked else {
        return Err(AppError::Other("已取消选择".into()));
    };
    let path = fp
        .into_path()
        .map_err(|e| AppError::Other(format!("读取文件路径失败：{e}")))?;
    let pem = std::fs::read_to_string(&path)?;
    let pem = pem.trim().to_string();
    if pem.is_empty() || !pem.contains("PRIVATE KEY") {
        return Err(AppError::Other("不是有效的私钥文件".into()));
    }
    // 验证私钥可解析（P1：不能只做字符串检查；带密码的私钥会被明确拒绝）
    validate_private_key(&pem)?;
    credential::set(config::CREDENTIAL_SSH_PRIVATE_KEY, &pem)?;

    // 仅用于本地确认展示（非 SSH 标准指纹）
    let digest = Sha256::digest(pem.as_bytes());
    let fp: String = digest.as_slice()[..8].iter().map(|b| format!("{b:02x}")).collect();
    Ok(ImportKeyResult {
        ok: true,
        key_fingerprint: Some(fp),
    })
}

/// 用 ssh-keygen -y 提取公钥验证私钥可解析；带密码的私钥在无交互下会被拒绝并明确提示。
fn validate_private_key(pem: &str) -> AppResult<()> {
    let tmp = tempfile::tempdir()?;
    let key_path = tmp.path().join("key");
    std::fs::write(&key_path, format!("{}\n", pem.trim_end()))?;
    fs_atomic::secure_permissions(&key_path)?;
    let out = ssh_keygen_command()
        .arg("-y")
        .arg("-f")
        .arg(&key_path)
        .output()
        .map_err(|e| AppError::Other(format!("无法校验私钥：{e}")))?;
    if !out.status.success() {
        let msg = String::from_utf8_lossy(&out.stderr);
        let hint = if msg.to_lowercase().contains("passphrase") || msg.to_lowercase().contains("encrypted") {
            "（暂不支持带密码的私钥，请使用无密码私钥）"
        } else {
            ""
        };
        return Err(AppError::Other(format!("私钥无效：{}{}", msg.trim(), hint)));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprint_is_openssh_format() {
        let fp = fingerprint(&[0u8; 32]);
        assert!(fp.starts_with("SHA256:"));
        assert_eq!(fp.len(), "SHA256:".len() + 43); // 32B 摘要 → 无 padding base64 43 字符
    }

    #[test]
    fn pinned_host_key_fingerprint_matches_constant() {
        // 自证：默认 secrets 里的 host key 公钥，其指纹必须等于编译期常量，否则 start 会失败关闭。
        let s = Secrets::default();
        let raw = decode_host_key(&s.host_key_b64).unwrap();
        assert_eq!(fingerprint(&raw), EXPECTED_HOST_KEY_FINGERPRINT);
    }

    #[test]
    fn synthesize_phase_attribution() {
        // ssh 死亡 → Reconnecting
        assert_eq!(synthesize_phase(false, true, false), TunnelPhase::Reconnecting);
        // 监听失效 → Reconnecting
        assert_eq!(synthesize_phase(true, false, false), TunnelPhase::Reconnecting);
        // API 故障 → Degraded（关键：不误重连 SSH）
        assert_eq!(synthesize_phase(true, true, true), TunnelPhase::Degraded);
        // 全绿 → Connecting（等待连续全绿）
        assert_eq!(synthesize_phase(true, true, false), TunnelPhase::Connecting);
    }

    #[test]
    fn backoff_grows_with_jitter_and_caps() {
        // 每档独立验证（抖动会累积，不能假设精确序列）：
        // 下一档 = min(2s, CAP) + 0~25% 抖动，且最终封顶 CAP。
        for s in [8u64, 16, 32, 64, 128, 256, 300, 400] {
            let v = next_backoff(s);
            let raw = (s * 2).min(BACKOFF_CAP_S);
            assert!(v >= raw, "退避不得低于翻倍基准（got {v}, raw {raw}, s {s}）");
            assert!(v <= BACKOFF_CAP_S, "退避必须封顶 {BACKOFF_CAP_S}（got {v}）");
            if raw < BACKOFF_CAP_S {
                assert!(v <= raw + raw / 4, "抖动上限 25%（got {v}, raw {raw}, s {s}）");
            }
        }
    }
}
