//! 实例探测命令面（§7.2 instance.*）：GET /api/status，失败时回不可达而非报错。

use serde::Serialize;

use crate::config;
use crate::ipc::CommandResult;
use crate::newapi::NewApiClient;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InstanceCapability {
    pub base_url: String,
    pub reachable: bool,
    pub version: String,
    pub server_address: String,
    pub register_enabled: bool,
    pub email_verification: bool,
    pub self_use_mode: bool,
}

#[tauri::command]
pub async fn instance_probe() -> CommandResult<InstanceCapability> {
    let client = NewApiClient::default_client();
    let cap = match client.status().await {
        Ok(st) => InstanceCapability {
            base_url: config::DEFAULT_BASE_URL.into(),
            reachable: true,
            version: st.version,
            server_address: st.server_address,
            register_enabled: st.register_enabled,
            email_verification: st.email_verification,
            self_use_mode: st.self_use_mode,
        },
        Err(_) => InstanceCapability {
            base_url: config::DEFAULT_BASE_URL.into(),
            reachable: false,
            version: String::new(),
            server_address: String::new(),
            register_enabled: false,
            email_verification: false,
            self_use_mode: false,
        },
    };
    CommandResult::ok(cap)
}
