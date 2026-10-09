//! 绿池（GreenPool / GP）装配层。只做装配：插件、窗口、状态，不含业务逻辑。
//! 分层依赖只向下：commands → services → newapi/adapters → 基础设施。

pub mod adapters;
pub mod commands;
pub mod config;
pub mod credential;
pub mod error;
pub mod fs_atomic;
pub mod ipc;
pub mod newapi;
pub mod services;
pub mod state;

use tauri::Manager;

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            if let Some(window) = app.get_webview_window(config::MAIN_WINDOW_LABEL) {
                let _ = window.show();
                let _ = window.set_focus();
            }
        }))
        // dev 默认 TRACE 会把海量窗口事件转发到 WebView 控制台，压到 Info。
        .plugin(tauri_plugin_log::Builder::new().level(log::LevelFilter::Info).build())
        .plugin(tauri_plugin_opener::init())
        // 应用内更新（GitHub Releases 分发；公钥在 tauri.conf.json plugins.updater.pubkey）
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.manage(state::AppState::new(data_dir));

            show_main_window(app.handle());

            // 冷启动自检（崩溃恢复）在 services 层实现后，经 app_status 暴露 pendingRecovery。
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::instance::instance_probe,
            commands::app::app_status,
            commands::app::app_open_external,
            commands::app::app_track_event,
            commands::install::install_detect,
            commands::install::install_plan,
            commands::install::install_apply,
            commands::install::install_rollback,
            commands::install::install_history,
            commands::credential::credential_status,
            commands::credential::credential_clear,
            commands::account::account_login,
            commands::account::account_logout,
            commands::account::account_status,
            commands::account::account_setup_key,
            commands::account::account_send_code,
            commands::account::account_register,
            commands::account::account_send_password_reset,
            commands::account::account_reset_password,
            commands::account::account_list_keys,
            commands::account::account_select_key,
            commands::account::account_link_status,
            commands::usage::usage_get_quota,
            commands::usage::usage_topups,
            commands::usage::usage_redemption_records,
            commands::usage::usage_detail,
            commands::shop::shop_status,
            commands::shop::shop_wallet,
            commands::shop::shop_send_bind_code,
            commands::shop::shop_bind,
            commands::shop::shop_sso_url,
            commands::shop::redeem_redeem,
        ])
        .run(tauri::generate_context!())
        .expect("error while running GreenPool");
}

/// 显示主窗口。骨架期直接显示；TODO(decoration)：改「装饰插件激活回调后显示 + 约 3s 强制显示超时兜底」。
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(config::MAIN_WINDOW_LABEL) {
        let _ = window.show();
    }
}
