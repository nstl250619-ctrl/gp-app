//! 用量查询编排：GET /api/user/self（余额）+ GET /api/log/self（明细/聚合）。

use serde::{Deserialize, Serialize};

use crate::config;
use crate::error::AppResult;
use crate::newapi::{LogItem, TopUpRecord};
use crate::services::shop;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct QuotaSummary {
    pub username: String,
    pub group: String,
    pub quota: i64,
    pub used_quota: i64,
    pub remaining: i64,
    pub request_count: i64,
    /// USD 计量（与 new-api 一致）：quota / 500000 = USD
    pub quota_usd: f64,
    pub used_usd: f64,
    pub remaining_usd: f64,
    /// 订阅余额（new-api 激活订阅的 amount_total - amount_used 之和，USD）；
    /// 无激活订阅为 None。展示口径：订阅余额优先于账户余额。
    #[serde(default)]
    pub subscription_usd: Option<f64>,
    /// 订阅配额总额（USD）
    #[serde(default)]
    pub subscription_total_usd: Option<f64>,
    /// 订阅已用（USD）
    #[serde(default)]
    pub subscription_used_usd: Option<f64>,
    /// 订阅到期时间（unix 秒；0/None=未知）
    #[serde(default)]
    pub subscription_end_time: Option<i64>,
}

pub async fn get_quota() -> AppResult<QuotaSummary> {
    // self 与订阅并行拉取（串行会多一个 RTT）
    let (s, sub) = tokio::join!(
        shop::gateway_self(),
        shop::gateway_subscription(),
    );
    let s = s?;
    // 订阅明细（尽力而为：接口失败视为无订阅）
    let sub = sub.ok().flatten();
    let subscription_usd = sub.as_ref().filter(|x| x.remaining() > 0).map(|x| config::quota_to_usd(x.remaining()));
    // new-api 的 self.quota 本身就是剩余余额（负数=已透支）
    Ok(QuotaSummary {
        username: s.username,
        group: s.group,
        quota: s.quota,
        used_quota: s.used_quota,
        remaining: s.quota,
        request_count: s.request_count,
        quota_usd: config::quota_to_usd(s.quota),
        used_usd: config::quota_to_usd(s.used_quota),
        remaining_usd: config::quota_to_usd(s.quota),
        subscription_usd,
        subscription_total_usd: sub.as_ref().map(|x| config::quota_to_usd(x.total)),
        subscription_used_usd: sub.as_ref().map(|x| config::quota_to_usd(x.used)),
        subscription_end_time: sub.as_ref().filter(|x| x.end_time > 0).map(|x| x.end_time),
    })
}

// ---- 兑换记录（logs type=1：兑换码核销不写 top_ups 表，从日志取） ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RedemptionRecord {
    pub created_at: i64,
    /// 日志原文（含「通过兑换码充值 ＄X 额度，兑换码ID N」）
    pub content: String,
    /// 从 content 解析出的 USD 金额（解析失败为 None）
    pub amount_usd: Option<f64>,
    /// 兑换码有效期（unix 秒；None=查询接口不可用；0=长期有效）
    pub expires_at: Option<i64>,
}

/// 兑换记录：logs type=1（topup 类）。兑换码核销不写 top_ups 表（那是支付充值），
/// 故从日志取，金额从 content 文本解析（＄X.XXXXXX，全角/半角 $ 均兼容）。
/// 有效期：按 content 里的兑换码 ID 并发查用户侧补丁接口；接口未上线时优雅降级为 None。
pub async fn redemption_records() -> AppResult<Vec<RedemptionRecord>> {
    // 走网关 /api/user/redemptions（type=1 日志代理）
    let items = shop::gateway_redemptions().await?;
    let mut out = Vec::new();
    // 仅保留兑换码核销日志（「通过兑换码充值 ＄X ...」）。
    // type=1 还包含「使用余额购买订阅成功」等扣费日志（content 无 ＄ 金额模式，
    // 解析为 None 会显示成空「+」），业务上不属于兑换记录，过滤掉。
    for l in items.into_iter().filter(|l| l.content.contains("兑换码")) {
        let amount_usd = parse_amount_usd(&l.content);
        out.push(RedemptionRecord {
            created_at: l.created_at,
            content: l.content,
            amount_usd,
            expires_at: None, // 网关暂无有效期补丁接口
        });
    }
    out.sort_by(|a, b| b.created_at.cmp(&a.created_at));
    Ok(out)
}

/// 从日志 content 解析 USD 金额（「＄0.001470」全角/半角 $ 均兼容，按 char_indices 处理宽度）
fn parse_amount_usd(content: &str) -> Option<f64> {
    for (idx, ch) in content.char_indices() {
        if ch == '＄' || ch == '$' {
            let rest: String = content[idx + ch.len_utf8()..]
                .chars()
                .take_while(|c| c.is_ascii_digit() || *c == '.')
                .collect();
            return rest.parse::<f64>().ok();
        }
    }
    None
}



/// 最近充值记录（走网关 /api/user/topups，最多 10 条）。
pub async fn topups() -> AppResult<Vec<TopUpRecord>> {
    shop::gateway_topups().await
}

// ---- 用量明细（时间范围聚合 + 日志列表） ----

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UsageDetail {
    pub items: Vec<LogItem>,
    /// 时间范围内 token 消耗总量（prompt + completion）。
    pub total_tokens: i64,
    /// 时间范围内调用次数。
    pub total_requests: i64,
    /// 时间范围内积分（quota 原始单位）消耗总量。
    pub total_quota: i64,
    /// 时间范围内费用（¥）。
    pub total_quota_usd: f64,
    /// 服务端匹配到的总条数（可能 > items.len()，因分页截断）。
    pub total: i64,
    /// 是否因分页上限截断。
    pub truncated: bool,
    /// 失败次数（type=5 错误日志，同时间范围，服务端 total 口径）。
    #[serde(default)]
    pub error_count: i64,
    /// 按天积分聚合（全量日志统计，不受明细 100 条限制；day 形如 "10/11"）。
    #[serde(default)]
    pub daily: Vec<DayStat>,
}

/// 单日积分消耗（柱状图数据点）。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DayStat {
    pub day: String,
    pub credits: f64,
}

/// 明细最多返回的条数（网关侧已截断；此处仅做截断标记口径）。
const DETAIL_MAX_ITEMS: i64 = 100;

/// 网关聚合返回（usage/summary）：明细前 100 + 全量口径的 总数/失败数/按天积分。
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GatewayUsageSummary {
    #[serde(default)]
    pub items: Vec<crate::newapi::LogItem>,
    #[serde(default)]
    pub total: i64,
    #[serde(default, alias = "error_count")]
    pub error_count: i64,
    #[serde(default)]
    pub daily: Vec<DayStat>,
}

/// 用量明细 + 全量聚合（走网关 /api/user/usage/summary，1 个请求）。
/// 翻页与按天聚合在网关侧完成（同机内网），桌面端不再 N 次往返。
pub async fn detail(start_ts: i64, end_ts: i64) -> AppResult<UsageDetail> {
    let s = shop::gateway_usage_summary(start_ts, end_ts).await?;
    let total_tokens: i64 = s.items.iter().map(|i| i.prompt_tokens + i.completion_tokens).sum();
    let total_quota: i64 = s.items.iter().map(|i| i.quota).sum();
    let truncated = s.total > DETAIL_MAX_ITEMS;

    Ok(UsageDetail {
        items: s.items,
        total_tokens,
        total_requests: s.total,
        total_quota,
        total_quota_usd: config::quota_to_usd(total_quota),
        total: s.total,
        truncated,
        error_count: s.error_count,
        daily: s.daily,
    })
}

/// unix 秒 → "月/日"（统一 UTC+8 口径，目标用户为中国时区）。
#[allow(dead_code)]
fn chrono_like_day(ts: i64) -> String {
    // 无依赖实现（避免引入 chrono）：civil-from-days 算法 + 固定 +8h 偏移
    const DAYS_PER_400Y: i64 = 146_097;
    let days = (ts + 8 * 3600).div_euclid(86_400);
    // Unix epoch 1970-01-01 是星期四；只用月/日，需要年内偏移：用 civil-from-days 算法
    let z = days + 719_468;
    let era = z.div_euclid(DAYS_PER_400Y);
    let doe = z.rem_euclid(DAYS_PER_400Y);
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let _year = era * 400 + yoe + (mp + 2) / 12;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let day = doy - (153 * mp + 2) / 5 + 1;
    format!("{}/{}", month, day)
}

