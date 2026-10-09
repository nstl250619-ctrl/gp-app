//! 命令层：参数校验 + 频率限制 + 转调 service + 组装信封。禁止直接发 HTTP / 读写文件。

pub mod account;
pub mod app;
pub mod credential;
pub mod install;
pub mod instance;
pub mod shop;
pub mod usage;
