//! 崩溃安全写入（ADR-010）：备份 → 原子写 → 权限收紧 → 回读校验。
//! Windows 权限用 ACL（icacls），因为 0o600 / Unix mode 在 Windows 无效（§21.2 #1）。

use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

use crate::error::{AppError, AppResult};

pub fn read_string(path: &Path) -> AppResult<String> {
    Ok(std::fs::read_to_string(path)?)
}

/// 备份为 `<原名>.bak.<时间戳>`（§11.5 / AC-08）。备份失败即中止。
/// 注意保留原扩展名（with_extension 会把 config.toml 变成 config.json.bak…，已被 Codex 当陌生文件清理）。
pub fn backup(path: &Path) -> AppResult<PathBuf> {
    if !path.exists() {
        return Err(AppError::BackupFailed(format!("文件不存在：{}", path.display())));
    }
    let ts = chrono::Local::now().format("%Y%m%d-%H%M%S");
    let name = path
        .file_name()
        .and_then(|s| s.to_str())
        .ok_or_else(|| AppError::BackupFailed(format!("路径非法：{}", path.display())))?;
    let bak = path.with_file_name(format!("{}.bak.{}", name, ts));
    std::fs::copy(path, &bak).map_err(|e| AppError::BackupFailed(e.to_string()))?;
    Ok(bak)
}

/// 写临时文件 → sync → 替换 → 收紧权限。
/// 注：Windows 上目标被占用时 rename 可能失败（§21.2 #12），调用方应有明确错误提示。
pub fn atomic_write(path: &Path, content: &str) -> AppResult<()> {
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
    tmp.write_all(content.as_bytes())?;
    tmp.as_file().sync_all()?;
    if path.exists() {
        std::fs::remove_file(path)?;
    }
    tmp.persist(path).map_err(|e| AppError::Other(e.to_string()))?;
    secure_permissions(path)?;
    Ok(())
}

#[cfg(windows)]
pub fn secure_permissions(path: &Path) -> AppResult<()> {
    use std::os::windows::process::CommandExt;
    // CREATE_NO_WINDOW：icacls 是控制台程序，GUI 进程每次写配置都启动它，
    // 不隐藏就会弹出黑色 Terminal 窗口（用户报告的「导入模型/启停隧道弹很多框」就是这个）。
    const CREATE_NO_WINDOW: u32 = 0x0800_0000;
    let user = std::env::var("USERNAME").unwrap_or_else(|_| ".".into());
    let out = std::process::Command::new("icacls")
        .creation_flags(CREATE_NO_WINDOW)
        .arg(path)
        .arg("/inheritance:r")
        .arg("/grant:r")
        .arg(format!("{user}:(F)"))
        .output()?;
    if out.status.success() {
        Ok(())
    } else {
        Err(AppError::Other(format!(
            "icacls 失败：{}",
            String::from_utf8_lossy(&out.stderr)
        )))
    }
}

#[cfg(not(windows))]
pub fn secure_permissions(path: &Path) -> AppResult<()> {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(crate::config::CONFIG_FILE_MODE))?;
    Ok(())
}

pub fn digest(path: &Path) -> AppResult<String> {
    let bytes = std::fs::read(path)?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let out = hasher.finalize();
    Ok(out.iter().map(|b| format!("{:02x}", b)).collect())
}

pub fn verify_digest(path: &Path, expected: &str) -> bool {
    digest(path).map(|d| d == expected).unwrap_or(false)
}
