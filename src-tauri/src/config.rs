//! 常量、路径、默认值（§11.6 与 §21 平台差异）。

use std::path::PathBuf;

pub const MAIN_WINDOW_LABEL: &str = "main";

pub const APP_NAME: &str = "绿池";
pub const APP_NAME_EN: &str = "GreenPool";
pub const APP_ABBR: &str = "GP";

pub const DEFAULT_BASE_URL: &str = "https://api.greenpool.cn";
/// Shop 商城（账号/钱包/充值唯一真源；2026-10-07 已上线接入）。
pub const SHOP_BASE_URL: &str = "https://shop.greenpool.cn";
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
pub const CREDENTIAL_ACCOUNT_TOKEN: &str = "newapi_access_token";
pub const CREDENTIAL_ACCOUNT_USERNAME: &str = "newapi_username";
pub const CREDENTIAL_ACCOUNT_USER_ID: &str = "newapi_user_id";
/// new-api 账号邮箱（绑定 Shop 用；登录标识可能是用户名，邮箱以站点 self 为准）。
pub const CREDENTIAL_ACCOUNT_EMAIL: &str = "newapi_email";
/// 登录密码（仅钥匙串）：new-api 会话令牌有过期时间，失效时静默重登换新。
pub const CREDENTIAL_ACCOUNT_PASSWORD: &str = "newapi_password";

// ---- WorkBuddy 配置写入常量（§11.6 原文） ----
pub const MANAGED_MODEL_NAME: &str = "GP";
pub const MANAGED_MODEL_ID_PREFIX: &str = "newapi-"; // 历史前缀，仅用于兼容识别
pub const MANAGED_MODEL_VENDOR: &str = "Custom"; // D-13
pub const MANAGED_MODEL_USE_CUSTOM_PROTOCOL: bool = false; // D-13
pub const MANAGED_MODEL_MAX_INPUT_TOKENS: u64 = 1_000_000; // D-13
pub const MANAGED_MODEL_MAX_OUTPUT_TOKENS: u64 = 1_000_000; // D-13
pub const REASONING_CAN_DISABLE_THINKING: bool = true; // D-11
pub const REASONING_DEFAULT_EFFORT: &str = "high"; // D-11
/// 思考档位集合（2026-10-09：CodeBuddy 列表徽标读取 reasoning.supportedEfforts，
/// 缺失则不显示思考模式 UI；defaultEffort 必须在集合内）。兜底用。
pub const REASONING_SUPPORTED_EFFORTS: &[&str] = &["low", "high", "xhigh"];

/// 模型 → 思考档位映射（2026-10-09 产品裁决：与 EP 系列实测口径逐模型对齐）。
/// (模型 id, 默认档位, 支持档位集合)。不在表内的模型走统一三档兜底。
pub const REASONING_EFFORTS_BY_MODEL: &[(&str, &str, &[&str])] = &[
    ("fast-model", "medium", &["medium"]),
    ("balanced-model", "medium", &["medium"]),
    ("deep-model", "medium", &["medium"]),
    ("deepseek-v4.1-flash", "high", &["low", "high", "max"]),
    ("deepseek-v4-pro", "high", &["high", "xhigh"]),
    ("glm-5.3", "high", &["low", "high", "max"]),
    ("glm-5.3-flash", "high", &["low", "high", "max"]),
    ("glm-5.2", "high", &["high", "xhigh"]),
    ("glm-5v-turbo", "medium", &["medium"]),
    ("minimax-m3", "medium", &["medium"]),
    ("kimi-k3-1", "high", &["low", "high", "xhigh"]),
    ("kimi-k2.8-preview", "high", &["low", "high", "max"]),
    ("kimi-k2.7", "medium", &["medium"]),
];

/// 按模型 id 查思考档位；未知模型回退统一三档 + high。
pub fn reasoning_effort_for(model_id: &str) -> (&'static str, &'static [&'static str]) {
    REASONING_EFFORTS_BY_MODEL
        .iter()
        .find(|(id, _, _)| *id == model_id)
        .map(|(_, d, e)| (*d, *e))
        .unwrap_or((REASONING_DEFAULT_EFFORT, REASONING_SUPPORTED_EFFORTS))
}

pub const WORKBUDDY_CONFIG_FILE: &str = "models.json";
pub const WORKBUDDY_DIR_ENV: &str = "WORKBUDDY_CONFIG_DIR";
pub const CODEBUDDY_DIR_ENV: &str = "CODEBUDDY_CONFIG_DIR";

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

// 临时默认模型清单（§8.6 实测 13 个）。todo(#6)：改为从 new-api GET /api/user/models 拉取。
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
fn candidate_paths(env_key: &str, dot_dir: &str) -> Vec<PathBuf> {
    let mut out = Vec::new();
    if let Ok(dir) = std::env::var(env_key) {
        let dir = dir.trim().to_string();
        if !dir.is_empty() {
            out.push(PathBuf::from(dir).join(WORKBUDDY_CONFIG_FILE));
        }
    }
    if let Some(home) = home_dir() {
        out.push(home.join(dot_dir).join(WORKBUDDY_CONFIG_FILE));
    }
    out
}

/// WorkBuddy：$WORKBUDDY_CONFIG_DIR/models.json → ~/.workbuddy/models.json。
pub fn workbuddy_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(WORKBUDDY_DIR_ENV, ".workbuddy")
}

/// CodeBuddy：$CODEBUDDY_CONFIG_DIR/models.json → ~/.codebuddy/models.json。
pub fn codebuddy_candidate_paths() -> Vec<PathBuf> {
    candidate_paths(CODEBUDDY_DIR_ENV, ".codebuddy")
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
