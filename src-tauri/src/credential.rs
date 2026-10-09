//! 凭据存取：只进系统钥匙串（ADR-004）。Windows = Credential Manager（keyring windows-native）。
//! 密钥/token 绝不出 Rust 侧；不可用时明确告知，绝不降级为明文。

use crate::config;
use crate::error::{AppError, AppResult};

fn entry(account: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(config::CREDENTIAL_SERVICE, account).map_err(AppError::from)
}

pub fn set(account: &str, secret: &str) -> AppResult<()> {
    entry(account)?.set_password(secret).map_err(AppError::from)
}

pub fn get(account: &str) -> AppResult<Option<String>> {
    match entry(account)?.get_password() {
        Ok(s) => Ok(Some(s)),
        Err(keyring::Error::NoEntry) => Ok(None),
        Err(e) => Err(AppError::from(e)),
    }
}

pub fn delete(account: &str) -> AppResult<()> {
    match entry(account)?.delete_credential() {
        Ok(()) => Ok(()),
        Err(keyring::Error::NoEntry) => Ok(()),
        Err(e) => Err(AppError::from(e)),
    }
}

/// 只读可用性探测（不触碰真实凭据）：构造成功且能读到探测项（NoEntry 视为可用，
/// 真正读写失败视为不可用）。修复：原先只判构造成功，钥匙串坏了也显示"可用"。
pub fn is_available() -> bool {
    match keyring::Entry::new(config::CREDENTIAL_SERVICE, "__probe__") {
        Ok(e) => matches!(e.get_password(), Ok(_) | Err(keyring::Error::NoEntry)),
        Err(_) => false,
    }
}
