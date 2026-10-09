// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

//! 桌面端入口：只调用装配层，不含任何业务逻辑。

fn main() {
    greenpool_lib::run();
}
