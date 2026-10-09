//! 用量查询编排：GET /api/user/self（余额）+ GET /api/log/self（明细/聚合）。

use serde::Serialize;

use crate::config;
use crate::error::{AppError, AppResult};
use crate::newapi::{LogItem, NewApiClient, TopUpRecord};
use crate::services::account;

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
}

pub async fn get_quota() -> AppResult<QuotaSummary> {
    let token = account::ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &token);
    let s = client.self_info().await?;
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
    let token = account::ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &token);
    let (items, _total) = client.logs_by_type(1, 1, 20, 0, 0).await?;
    let mut handles = Vec::new();
    for l in items.into_iter().filter(|l| !l.content.is_empty()) {
        let c = client.clone();
        handles.push(tokio::spawn(async move {
            let amount_usd = parse_amount_usd(&l.content);
            let rid = parse_redemption_id(&l.content);
            let expires_at = match rid {
                Some(id) => c.redemption_expired_time(id).await,
                None => None,
            };
            RedemptionRecord { created_at: l.created_at, content: l.content, amount_usd, expires_at }
        }));
    }
    let mut out = Vec::new();
    for h in handles {
        if let Ok(r) = h.await {
            out.push(r);
        }
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

/// 从日志 content 解析兑换码 ID（「…，兑换码ID 42」）
fn parse_redemption_id(content: &str) -> Option<i64> {
    let marker = "兑换码ID";
    let idx = content.find(marker)?;
    let rest = &content[idx + marker.len()..];
    let num: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num.parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_redemption_id_extracts_number() {
        assert_eq!(parse_redemption_id("通过兑换码充值 ＄10 额度，兑换码ID 42"), Some(42));
        assert_eq!(parse_redemption_id("兑换码ID 7"), Some(7));
        assert_eq!(parse_redemption_id("没有ID的日志"), None);
        assert_eq!(parse_redemption_id("兑换码ID abc"), None);
    }
}

/// 最近充值记录（来自站点，最多 10 条）。
pub async fn topups() -> AppResult<Vec<TopUpRecord>> {
    let token = account::ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &token);
    client.topup_records().await
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
}

/// 安全上限：2 万条（200 页 × 100/页，站点 page_size 硬上限 100）。
const MAX_PAGES: i64 = 200;
const PAGE_SIZE: i64 = 100;

/// 用量明细 + 聚合（type=2 计费日志，按时间范围过滤）。
/// 拉全窗口内所有页（站点 page_size 上限 100，并发拉取），统计与明细均为真值。
pub async fn detail(start_ts: i64, end_ts: i64) -> AppResult<UsageDetail> {
    let token = account::ensure_session().await?;
    let client = NewApiClient::with_token(config::DEFAULT_BASE_URL, &token);

    // 第一页：拿 total 决定还要并发拉几页
    let (first_items, total) = client.logs(1, PAGE_SIZE, start_ts, end_ts).await?;
    let mut all_items = first_items;
    let total_pages = (((total as f64) / (PAGE_SIZE as f64)).ceil() as i64).min(MAX_PAGES);
    let truncated = total > total_pages * PAGE_SIZE;

    if total_pages > 1 {
        let mut handles = Vec::new();
        for p in 2..=total_pages {
            let c = client.clone();
            handles.push(tokio::spawn(async move { c.logs(p, PAGE_SIZE, start_ts, end_ts).await }));
        }
        for h in handles {
            let (items, _) = h
                .await
                .map_err(|_| AppError::Other("日志拉取任务失败".into()))??;
            all_items.extend(items);
        }
    }

    let total_tokens: i64 = all_items.iter().map(|i| i.prompt_tokens + i.completion_tokens).sum();
    // 真实窗口调用次数 = 服务端计数（不再用已拉取条数近似）
    let total_requests = total;
    let total_quota: i64 = all_items.iter().map(|i| i.quota).sum();

    Ok(UsageDetail {
        items: all_items,
        total_tokens,
        total_requests,
        total_quota,
        total_quota_usd: config::quota_to_usd(total_quota),
        total,
        truncated,
    })
}

