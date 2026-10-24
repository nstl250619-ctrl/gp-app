//! 常量、路径、默认值（§11.6 与 §21 平台差异）。

use std::path::PathBuf;

pub const MAIN_WINDOW_LABEL: &str = "main";

pub const APP_NAME: &str = "绿池";
pub const APP_NAME_EN: &str = "GreenPool";
pub const APP_ABBR: &str = "GP";

pub const DEFAULT_BASE_URL: &str = "https://api.greenpool.cn";
/// GP 自身状态探测地址（顶部/底部「中转站已连接」绿点）。
/// 用 aigw：灰云直连（不经 Cloudflare），国内探测更快更稳；
/// 注意与 DEFAULT_BASE_URL 分离——后者仍是一键配置写入客户端的默认服务地址。
pub const PROBE_BASE_URL: &str = "https://aigw.greenpool.cn";
/// Shop 商城（账号/钱包/充值唯一真源；2026-10-07 已上线接入）。
/// Shop 网关（账号/钱包/充值/订单/商城/核销/SSO 全部经此）。
/// 用 shop-direct：灰云直连（不经 Cloudflare），实测同一服务、延迟 0.29s vs 2.80s。
pub const SHOP_BASE_URL: &str = "https://shop-direct.greenpool.cn";
/// Shop 固定邀请码（产品配置；注册时自动带入，用户无感）。
pub const SHOP_INVITE_CODE: &str = "DYQFMH";
/// 设备会话 token（7.1 建立，匿名，长期复用）。
pub const SHOP_SESSION_TOKEN: &str = "shop_session_token";
/// Shop 登录态 token（7.3/7.4 返回，Bearer 用于 7.5+）。
pub const SHOP_AUTH_TOKEN: &str = "shop_auth_token";
pub const SHOP_USER_ID: &str = "shop_user_id";
/// Shop 登录密码（绑定时的自定义密码；与 new-api 密码不同时由用户提供，持久化供后续静默重登）。
pub const SHOP_PASSWORD: &str = "shop_password";

/// 额度换算（USD 口径，与 new-api 一致）：quota / QUOTA_PER_UNIT = USD。
/// new-api 已全面采用 USD 计量（QUOTA_PER_UNIT=500000 quota = $1），工具端跟随，不再做汇率折算。
pub const QUOTA_PER_UNIT: f64 = 500_000.0;

pub fn quota_to_usd(quota: i64) -> f64 {
    quota as f64 / QUOTA_PER_UNIT
}
pub const CREDENTIAL_SERVICE: &str = "cn.greenpool.gp";
pub const CREDENTIAL_ACCOUNT_API_KEY: &str = "newapi_api_key";
/// 当前选中的 new-api 令牌 id（密钥卡下拉选择的落点）。
pub const CREDENTIAL_ACCOUNT_KEY_TOKEN_ID: &str = "newapi_key_token_id";
/// DSH 专属 key 的明文（独立钥匙串条目，与通用 key 分离；DSH IDE 卡导入时使用）。
pub const CREDENTIAL_DSH_API_KEY: &str = "dsh_api_key";
/// DSH 专属令牌名（new-api 侧分组/令牌约定名，注册后自动创建；按名字识别）。
pub const DSH_TOKEN_NAME: &str = "DSH-Only";
/// DSH 专属分组名（new-api 侧分组码，与令牌名同名但语义独立：用于 ?group= 查询与复用校验）。
pub const DSH_GROUP: &str = "DSH-Only";
pub const CREDENTIAL_ACCOUNT_TOKEN: &str = "newapi_access_token";
pub const CREDENTIAL_ACCOUNT_USERNAME: &str = "newapi_username";
pub const CREDENTIAL_ACCOUNT_USER_ID: &str = "newapi_user_id";
/// new-api 账号邮箱（绑定 Shop 用；登录标识可能是用户名，邮箱以站点 self 为准）。
pub const CREDENTIAL_ACCOUNT_EMAIL: &str = "newapi_email";
/// 登录密码（仅钥匙串）：new-api 会话令牌有过期时间，失效时静默重登换新。
pub const CREDENTIAL_ACCOUNT_PASSWORD: &str = "newapi_password";

// ---- 隧道子系统（GPHub 核心增量迁入）----
/// SSH 私钥存钥匙串（内嵌私钥 → keyring，绝不再进 exe/源码）。
pub const CREDENTIAL_SSH_PRIVATE_KEY: &str = "ssh_private_key";
/// Tauri IPC 事件通道（替代 GPHub 的 8737 HTTP 推送）。
pub const TUNNEL_EVENT_STATUS: &str = "tunnel://status";
pub const TUNNEL_EVENT_LOG: &str = "tunnel://log";
/// 隧道本地转发地址（产品决策：隧道运行中时一键配置的 base_url 静默替换为它，对用户透明）。
pub const TUNNEL_BASE_URL: &str = "http://127.0.0.1:3000";
/// 直链加速总开关（产品决策）：false = 功能未启用（前端按钮禁用 + 后端 start 拒绝 + SSH 凭据占位）。
/// 上线时改 true 并恢复 secrets.rs / tunnel.rs / secrets.enc 的真实凭据。
pub const TURBO_ENABLED: bool = true;

// ---- WorkBuddy 配置写入常量（§11.6 原文） ----
pub const MANAGED_MODEL_NAME: &str = "GP";
pub const MANAGED_MODEL_ID_PREFIX: &str = "newapi-"; // 历史前缀，仅用于兼容识别
pub const MANAGED_MODEL_VENDOR: &str = "Custom"; // D-13
pub const MANAGED_MODEL_USE_CUSTOM_PROTOCOL: bool = false; // D-13
pub const MANAGED_MODEL_MAX_INPUT_TOKENS: u64 = 1_000_000; // D-13
pub const MANAGED_MODEL_MAX_OUTPUT_TOKENS: u64 = 1_000_000; // D-13
pub const REASONING_ONLY_REASONING: bool = true; // D-11: 默认开启思考模式（EP 系列同款，缺此字段客户端默认关闭）
pub const REASONING_DEFAULT_EFFORT: &str = "high"; // D-11: 默认档位「高」
/// 思考档位集合（2026-10-09：CodeBuddy 列表徽标读取 reasoning.supportedEfforts，
/// 缺失则不显示思考模式 UI；defaultEffort 必须在集合内）。
pub const REASONING_SUPPORTED_EFFORTS: &[&str] = &["low", "high", "xhigh"];

pub const WORKBUDDY_CONFIG_FILE: &str = "models.json";
pub const WORKBUDDY_DIR_ENV: &str = "WORKBUDDY_CONFIG_DIR";
pub const CODEBUDDY_DIR_ENV: &str = "CODEBUDDY_CONFIG_DIR";
/// Codex CLI：配置根 = $CODEX_HOME（官方 env），配置文件 config.toml。
pub const CODEX_CONFIG_FILE: &str = "config.toml";
pub const CODEX_DIR_ENV: &str = "CODEX_HOME";
/// Claude Code：$CLAUDE_CONFIG_DIR/settings.json → ~/.claude/settings.json。
pub const CLAUDE_CONFIG_FILE: &str = "settings.json";
pub const CLAUDE_DIR_ENV: &str = "CLAUDE_CONFIG_DIR";
/// Hermes：$HERMES_CONFIG_DIR/config.yaml → ~/.hermes/config.yaml。
pub const HERMES_CONFIG_FILE: &str = "config.yaml";
pub const HERMES_DIR_ENV: &str = "HERMES_CONFIG_DIR";
/// DeepSeek Harness：$DSH_HOME/profiles/desktop/cordis.patch.yml → ~/.dsh/…。
pub const DSH_PATCH_FILE: &str = "profiles/desktop/cordis.patch.yml";
pub const DSH_CREDENTIALS_FILE: &str = ".credentials.yaml";
pub const DSH_DIR_ENV: &str = "DSH_HOME";

// ⚠️ Unix-only。Windows 上 0o600 无效，须用 ACL（icacls）收紧，见 fs_atomic::secure_permissions。
#[cfg(unix)]
pub const CONFIG_FILE_MODE: u32 = 0o600;

// relatedModels 关键词启发式（§11.6）
pub const RELATED_LITE_KEYWORDS: &[&str] = &[
    "mini", "flash", "lite", "haiku", "small", "nano", "tiny", "turbo", "fast", "8b", "4b",
];
pub const RELATED_REASONING_KEYWORDS: &[&str] = &[
    "o1", "o3", "o4", "reason", "thinking", "qwq", "deepseek-r", "opus", "sonnet", "max", "pro",
    "235b", "671b",
];

// 模型清单离线兜底（§8.6 实测 13 个）：仅在网关不可达 / 拉取模型失败时启用，
// 正常路径始终走 install.available_models → 网关 GET /api/user/models（按用户分组过滤）。
pub const DEFAULT_MODEL_IDS: &[&str] = &[
    "fast-model",
    "balanced-model",
    "deep-model",
    "deepseek-v4.1-flash",
    "deepseek-v4-pro",
    "glm-5.3",
    "glm-5.3-flash",
    "glm-5.2",
    "glm-5v-turbo",
    "minimax-m3",
    "kimi-k3-1",
    "kimi-k2.8-preview",
    "kimi-k2.7",
];

/// 配置落点候选（§11.1 探测优先级）：环境变量覆盖优先，其次用户主目录约定位置。
/// Windows 用 USERPROFILE 而非 HOME（§21.2 #2）。两平台各自独立，绝不互为 fallback。
fn candidate_paths(env_key: &str, dot_dir: &str, file: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(dir) = std::env::var(env_key) {
        let dir = dir.trim().to_string();
        if !dir.is_empty() {
            out.push(PathBuf::from(dir).join(file));
        }
    }
    if let Some(home) = home_dir() {
        out.push(home.join(dot_dir).join(file));
    }
    out
}

/// WorkBuddy：$WORKBUDDY_CONFIG_DIR/models.json → ~/.workbuddy/models.json。
pub fn workbuddy_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(WORKBUDDY_DIR_ENV, ".workbuddy", WORKBUDDY_CONFIG_FILE)
}

/// CodeBuddy：$CODEBUDDY_CONFIG_DIR/models.json → ~/.codebuddy/models.json。
pub fn codebuddy_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(CODEBUDDY_DIR_ENV, ".codebuddy", WORKBUDDY_CONFIG_FILE)
}

/// Codex：$CODEX_HOME/config.toml → ~/.codex/config.toml。
pub fn codex_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(CODEX_DIR_ENV, ".codex", CODEX_CONFIG_FILE)
}

/// Claude Code：$CLAUDE_CONFIG_DIR/settings.json → ~/.claude/settings.json。
pub fn claude_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(CLAUDE_DIR_ENV, ".claude", CLAUDE_CONFIG_FILE)
}

/// OpenClaw：$OPENCLAW_CONFIG_DIR → ~/.openclaw/（openclaw.json 优先，models.json 兜底）。
pub fn openclaw_candidate_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(dir) = std::env::var("OPENCLAW_CONFIG_DIR") {
        let dir = dir.trim().to_string();
        if !dir.is_empty() {
            let base = PathBuf::from(&dir);
            out.push(base.join("openclaw.json"));
            out.push(base.join("models.json"));
        }
    }
    if let Some(home) = home_dir() {
        out.push(home.join(".openclaw").join("openclaw.json"));
        out.push(home.join(".openclaw").join("models.json"));
    }
    out
}

/// Hermes：$HERMES_CONFIG_DIR/config.yaml → ~/.hermes/config.yaml。
pub fn hermes_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(HERMES_DIR_ENV, ".hermes", HERMES_CONFIG_FILE)
}

/// DeepSeek Harness：$DSH_HOME/profiles/desktop/cordis.patch.yml → ~/.dsh/…。
/// 注意 DSH_HOME 指向 profile 根（patch 路径带子目录），与 candidate_paths 的 join(file) 模型不同。
pub fn dsh_candidate_paths() -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(dir) = std::env::var(DSH_DIR_ENV) {
        let dir = dir.trim().to_string();
        if !dir.is_empty() {
            out.push(PathBuf::from(&dir).join(DSH_PATCH_FILE));
        }
    }
    if let Some(home) = home_dir() {
        out.push(home.join(".dsh").join(DSH_PATCH_FILE));
    }
    out
}

fn home_dir() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE").ok().map(PathBuf::from)
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME").ok().map(PathBuf::from)
    }
}
