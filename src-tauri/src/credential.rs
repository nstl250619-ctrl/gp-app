//! 凭据存取：只进系统钥匙串（ADR-004）。Windows = Credential Manager（keyring windows-native）。
//! 密钥/token 绝不出 Rust 侧；不可用时明确告知，绝不降级为明文。
//!
//! 性能（2026-10-18）：钥匙串是同步系统调用（单次 ~10-50ms），而 get 高频出现在每次网关请求
//! 与状态探测路径（app.status 一次 4 读、gateway_token 每请求 1 读）。此处加进程内缓存：
//! - get 命中缓存直接返回（不再进系统钥匙串）
//! - set / delete 同步更新缓存，保证跨账号切换、登出后立即可见
//! - is_available() 保持实时探测（不缓存，用于诊断钥匙串是否损坏）

use std::collections::HashMap;
use std::sync::Mutex;

use crate::config;
use crate::error::{AppError, AppResult};

/// 进程内凭据缓存：account → Option<secret>（None 表示「确认不存在」）。
/// 注意区分「未缓存」与「确认不存在」：外层 Option 表示缓存是否已初始化。
static CACHE: Mutex<Option<HashMap<String, Option<String>>>> = Mutex::new(None);

fn cached(account: &str) -> Option<Option<String>> {
    CACHE
        .lock()
        .ok()
        .and_then(|m| m.as_ref().and_then(|map| map.get(account).cloned()))
}

fn put(account: &str, value: Option<String>) {
    if let Ok(mut guard) = CACHE.lock() {
        guard
            .get_or_insert_with(HashMap::new)
            .insert(account.to_string(), value);
    }
}

fn entry(account: &str) -> AppResult<keyring::Entry> {
    keyring::Entry::new(config::CREDENTIAL_SERVICE, account).map_err(AppError::from)
}

pub fn set(account: &str, secret: &str) -> AppResult<()> {
    entry(account)?.set_password(secret).map_err(AppError::from)?;
    put(account, Some(secret.to_string()));
    Ok(())
}

pub fn get(account: &str) -> AppResult<Option<String>> {
    if let Some(v) = cached(account) {
        return Ok(v);
    }
    let v = match entry(account)?.get_password() {
        Ok(s) => Some(s),
        Err(keyring::Error::NoEntry) => None,
        Err(e) => return Err(AppError::from(e)),
    };
    put(account, v.clone());
    Ok(v)
}

pub fn delete(account: &str) -> AppResult<()> {
    match entry(account)?.delete_credential() {
        Ok(()) => {}
        Err(keyring::Error::NoEntry) => {}
        Err(e) => return Err(AppError::from(e)),
    }
    put(account, None);
    Ok(())
}

/// 只读可用性探测（不触碰真实凭据）：构造成功且能读到探测项（NoEntry 视为可用，
/// 真正读写失败视为不可用）。修复：原先只判构造成功，钥匙串坏了也显示"可用"。
/// 保持实时（不走缓存），用于诊断钥匙串健康。
pub fn is_available() -> bool {
    match keyring::Entry::new(config::CREDENTIAL_SERVICE, "__probe__") {
        Ok(e) => matches!(e.get_password(), Ok(_) | Err(keyring::Error::NoEntry)),
        Err(_) => false,
    }
}
