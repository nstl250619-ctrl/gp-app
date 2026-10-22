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
pub mod tray;

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
        .plugin(tauri_plugin_dialog::init())
        // 应用内更新（GitHub Releases 分发；公钥在 tauri.conf.json plugins.updater.pubkey）
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .unwrap_or_else(|_| std::path::PathBuf::from("."));
            app.manage(state::AppState::new(data_dir.clone()));
            tray::init(app);

            // 加密资源分发：首次启动把 resource_dir/secrets.enc 复制到 data_dir（若 data_dir 尚无）
            if let Ok(res_dir) = app.path().resource_dir() {
                let src = res_dir.join("secrets.enc");
                let dst = data_dir.join("secrets.enc");
                if src.exists() && !dst.exists() {
                    let _ = std::fs::copy(&src, &dst);
                }
            }

            show_main_window(app.handle());

            // 冷启动自检（崩溃恢复）在 services 层实现后，经 app_status 暴露 pendingRecovery。
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                // 关闭行为切换：隧道运行中 → 隐藏到托盘驻守（不退出）；隧道停止 → 默认关闭退出。
                let data_dir = window
                    .app_handle()
                    .state::<state::AppState>()
                    .data_dir
                    .clone();
                let phase = crate::services::tunnel::status(&data_dir).phase;
                if phase != crate::services::tunnel::TunnelPhase::Stopped {
                    api.prevent_close();
                    let _ = window.hide();
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::instance::instance_probe,
            commands::app::app_status,
            commands::app::app_open_external,
            commands::app::app_track_event,
            commands::install::install_detect,
            commands::install::install_available_models,
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
            commands::account::account_setup_dsh_key,
            commands::account::account_bind_newapi,
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
            commands::shop::shop_products,
            commands::shop::shop_product_detail,
            commands::shop::shop_buy_order,
            commands::shop::shop_my_orders,
            commands::shop::shop_topup_create,
            commands::shop::shop_topup_status,
            commands::tunnel::tunnel_status,
            commands::tunnel::tunnel_start,
            commands::tunnel::tunnel_stop,
            commands::tunnel::tunnel_restart,
            commands::tunnel::tunnel_probe,
            commands::tunnel::tunnel_import_key,
        ])
        .build(tauri::generate_context!())
        .expect("error while building GreenPool")
        .run(|app_handle, event| {
            // 应用退出：先改回直连 base_url，再同步清理隧道（进程 + 监听端口 + 临时私钥）。
            // 后台强杀不触发此钩子 → base_url 停留隧道地址，产品裁决「可接受」。
            if let tauri::RunEvent::Exit = event {
                let data_dir = app_handle.state::<state::AppState>().data_dir.clone();
                let _ = crate::services::install::rewrite_base_urls(&data_dir, false);
                let _ = crate::services::tunnel::stop(app_handle);
            }
        });
}

/// 显示主窗口。骨架期直接显示；TODO(decoration)：改「装饰插件激活回调后显示 + 约 3s 强制显示超时兜底」。
fn show_main_window(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window(config::MAIN_WINDOW_LABEL) {
        let _ = window.show();
    }
}
