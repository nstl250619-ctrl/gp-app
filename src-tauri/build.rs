// 把自定义命令收口到 ACL：tauri-build 会为每个命令生成 allow-<command> / deny-<command> 权限。
// 新增命令必须同步三处：本文件 / capabilities/default.json / lib.rs 的 generate_handler!。
const APP_COMMANDS: &[&str] = &[
    "instance_probe",
    "app_status",
    "app_open_external",
    "app_track_event",
    "install_detect",
    "install_plan",
    "install_apply",
    "install_rollback",
    "install_history",
    "credential_status",
    "credential_clear",
    "account_login",
    "account_logout",
    "account_status",
    "account_setup_key",
    "account_send_code",
    "account_register",
    "account_send_password_reset",
    "account_reset_password",
    "account_list_keys",
    "account_select_key",
    "account_link_status",
    "usage_get_quota",
    "usage_topups",
    "usage_redemption_records",
    "usage_detail",
    "shop_status",
    "shop_wallet",
    "shop_send_bind_code",
    "shop_bind",
    "shop_sso_url",
    "redeem_redeem",
];

fn main() {
    tauri_build::try_build(
        tauri_build::Attributes::new()
            .app_manifest(tauri_build::AppManifest::new().commands(APP_COMMANDS)),
    )
    .expect("failed to run tauri-build");
}
