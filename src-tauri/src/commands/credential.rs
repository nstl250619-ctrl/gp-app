//! 凭据命令面（§7.2 credential.*）：只回状态，绝不回明文。

use serde::Serialize;

use crate::config;
use crate::credential;
use crate::ipc::{CommandResult, SimpleAck};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CredentialStatus {
    pub available: bool,
    pub has_api_key: bool,
}

#[tauri::command]
pub fn credential_status() -> CommandResult<CredentialStatus> {
    let available = credential::is_available();
    let has_api_key = credential::get(config::CREDENTIAL_ACCOUNT_API_KEY)
        .map(|o| o.is_some())
        .unwrap_or(false);
    CommandResult::ok(CredentialStatus { available, has_api_key })
}

#[tauri::command]
pub fn credential_clear() -> CommandResult<SimpleAck> {
    match credential::delete(config::CREDENTIAL_ACCOUNT_API_KEY) {
        Ok(()) => CommandResult::ok(SimpleAck),
        Err(e) => CommandResult::err(crate::ipc::CommandError::from(e)),
    }
}
