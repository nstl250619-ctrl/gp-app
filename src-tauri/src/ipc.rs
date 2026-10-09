//! IPC 统一信封（§7.1）：成功 { ok: true, data }，失败 { ok: false, error }。
//! 字段名按前端契约输出 camelCase。

use serde::Serialize;

#[derive(Debug, Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CommandError {
    pub code: String,
    pub category: String,
    pub message: String,
    pub next_action: Option<String>,
    pub detail: Option<String>,
}

#[derive(Debug, Serialize)]
pub struct CommandResult<T> {
    pub ok: bool,
    pub data: Option<T>,
    pub error: Option<CommandError>,
}

/// 无数据确认（SimpleAck，§7.2）。
#[derive(Debug, Serialize)]
pub struct SimpleAck;

impl<T: Serialize> CommandResult<T> {
    pub fn ok(data: T) -> Self {
        Self { ok: true, data: Some(data), error: None }
    }

    pub fn err(error: CommandError) -> Self {
        Self { ok: false, data: None, error: Some(error) }
    }
}

/// 命令层通用包装：AppResult -> 信封。
pub fn respond<T: Serialize>(r: crate::error::AppResult<T>) -> CommandResult<T> {
    match r {
        Ok(v) => CommandResult::ok(v),
        Err(e) => CommandResult::err(CommandError::from(e)),
    }
}

impl From<crate::error::AppError> for CommandError {
    fn from(e: crate::error::AppError) -> Self {
        // 骨架期最小映射；后续按 §14 的 code -> 人话 + nextAction 映射表细化。
        let (code, category, message) = match &e {
            crate::error::AppError::Network(_) => ("NETWORK", "network", e.to_string()),
            crate::error::AppError::KeyMissing => ("KEY_MISSING", "user", e.to_string()),
            crate::error::AppError::EmailTaken => ("EMAIL_TAKEN", "user", e.to_string()),
            crate::error::AppError::SessionExpired => ("AUTH_EXPIRED", "user", e.to_string()),
            crate::error::AppError::TargetNotFound => ("TARGET_NOT_FOUND", "user", e.to_string()),
            crate::error::AppError::BackupFailed(_) => ("FILE_BACKUP_FAILED", "server", e.to_string()),
            crate::error::AppError::RollbackFailed(_) => ("ROLLBACK_FAILED", "server", e.to_string()),
            crate::error::AppError::Keyring(_) => ("KEYCHAIN_UNAVAILABLE", "server", e.to_string()),
            _ => ("INTERNAL", "server", e.to_string()),
        };
        CommandError {
            code: code.into(),
            category: category.into(),
            message,
            next_action: None,
            detail: None,
        }
    }
}
