//! 业务层：编排。依赖只向下，禁止依赖 commands / 前端类型。

pub mod account;
pub mod install;
pub mod secrets;
pub mod shop;
pub mod tunnel;
pub mod usage;

/// 共享 reqwest Client 构造器（去重：shop / newapi 两处原先各自 builder，逻辑一致）。
/// - connect_timeout：建连快速失败，避免界面干等
/// - timeout：总超时上限（shop 走网关 15s；newapi 直连 30s）
/// - pool_max_idle_per_host：复用空闲连接
/// - no_proxy：系统代理可能错误路由 *.greenpool.cn（SSO 铸号被劫走），实测直连正常
///
/// 说明：不同域名（shop.greenpool.cn / api.greenpool.cn）的连接池本就按 host 隔离，
/// 因此这里只统一「构造逻辑」，不强行合并两个单例（合并无收益反而互相牵制超时）。
pub(crate) fn http_client(connect_timeout: u64, timeout: u64) -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(std::time::Duration::from_secs(connect_timeout))
        .timeout(std::time::Duration::from_secs(timeout))
        .pool_max_idle_per_host(8)
        .no_proxy()
        .build()
        .expect("greenpool http client")
}
