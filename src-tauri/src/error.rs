//! 统一错误枚举（§14）。命令层据此映射成 CommandError 信封。

use thiserror::Error;

pub type AppResult<T> = Result<T, AppError>;

#[derive(Debug, Error)]
pub enum AppError {
    #[error("网络连不上中转站")]
    Network(#[from] reqwest::Error),

    #[error("文件读写失败：{0}")]
    Io(#[from] std::io::Error),

    #[error("系统钥匙串不可用：{0}")]
    Keyring(#[from] keyring::Error),

    #[error("配置解析失败：{0}")]
    Json(#[from] serde_json::Error),

    #[error("未找到密钥，请先完成兑换")]
    KeyMissing,

    #[error("没找到目标工具的配置")]
    TargetNotFound,

    #[error("该邮箱已注册，请直接登录")]
    EmailTaken,

    #[error("登录已过期，请重新登录")]
    SessionExpired,

    #[error("备份原配置失败，已中止写入：{0}")]
    BackupFailed(String),

    #[error("写入校验未通过：{0}")]
    VerifyFailed(String),

    #[error("还原失败：{0}")]
    RollbackFailed(String),

    #[error("{0}")]
    Other(String),
}
