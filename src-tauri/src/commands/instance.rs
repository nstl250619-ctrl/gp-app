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
    // 双地址兜底：aigw（灰云直连，主）失败时回退 api（Cloudflare），任一可达即视为在线——
    // 避免单条线路故障时绿点误报「不可达」。返回的 base_url 即实际可达的那个。
    for base in [config::PROBE_BASE_URL, config::DEFAULT_BASE_URL] {
        let client = NewApiClient::new(base);
        if let Ok(st) = client.status().await {
            return CommandResult::ok(InstanceCapability {
                base_url: base.into(),
                reachable: true,
                version: st.version,
                server_address: st.server_address,
                register_enabled: st.register_enabled,
                email_verification: st.email_verification,
                self_use_mode: st.self_use_mode,
            });
        }
    }
    CommandResult::ok(InstanceCapability {
        base_url: config::PROBE_BASE_URL.into(),
        reachable: false,
        version: String::new(),
        server_address: String::new(),
        register_enabled: false,
        email_verification: false,
        self_use_mode: false,
    })
}
