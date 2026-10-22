//! 系统托盘：常驻图标（应用启动创建一次，退出随进程消失）。
//!
//! 交互约定（产品决策）：
//!   - 隧道运行中：关闭窗口 → 隐藏到托盘（驻守维持隧道端口），不退出。
//!   - 隧道停止：关闭窗口 → 正常退出（托盘随进程消失）。
//!   - 托盘左键：唤起主窗口；右键菜单：退出工具（停隧道 + 退出）。
//!
//! 为什么常驻而非动态创建/移除：Tauri 2 在部分路径下 drop TrayIcon 不能可靠移除
//! 系统托盘图标，导致「停止→再启动」图标累积；常驻方案图标恒定 1 个，无此问题。

use tauri::{
    menu::{Menu, MenuItem},
    tray::{MouseButton, TrayIconBuilder, TrayIconEvent},
    Manager,
};

/// 注册常驻托盘（setup 时调用一次）。
pub fn init(app: &tauri::App) {
    let Ok(quit) = MenuItem::with_id(app, "quit", "退出工具", true, None::<&str>) else {
        return;
    };
    let Ok(menu) = Menu::with_items(app, &[&quit]) else {
        return;
    };

    let mut builder = TrayIconBuilder::with_id("main-tray")
        .tooltip("绿池 GreenPool")
        .menu(&menu)
        .show_menu_on_left_click(false) // 左键唤起窗口，右键弹菜单
        .on_menu_event(|app, event| {
            if event.id() == "quit" {
                // 显式停隧道（RunEvent::Exit 也会兜底），再退出
                let _ = crate::services::tunnel::stop(app);
                app.exit(0);
            }
        })
        .on_tray_icon_event(|tray, event| {
            if let TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } = event
            {
                if let Some(w) = tray.app_handle().get_webview_window(crate::config::MAIN_WINDOW_LABEL) {
                    let _ = w.show();
                    let _ = w.set_focus();
                }
            }
        });
    if let Some(icon) = app.default_window_icon() {
        builder = builder.icon(icon.clone());
    }
    let _ = builder.build(app);
}
