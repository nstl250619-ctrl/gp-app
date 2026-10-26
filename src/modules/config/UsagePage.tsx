// 用量页：4 张等高统计卡（统一三段结构：标签 → 大数字 → 一行紧凑小字）+ 统计/明细分区。
import { useCallback, useEffect, useState, type CSSProperties } from 'react';
import { errorMessage, invokeCommand, isIpcError } from '@/services/ipc';
import type { QuotaSummary, SubscriptionCard, UsageDetail } from '@/types/ipc';
import { t } from '@/i18n';
import { fmtCredits, fmtUsdFloor2 } from '@/utils/fmt';
import { formatDateTime } from '@/utils/time';
import UsageDetailSection, { rangeLabel, rangeTimestamps, type TimeRange } from './UsageDetail';
import BindNewapiDialog from './BindNewapiDialog';

/** Shop requires binding a new-api account (contract 4.1): open bind dialog instead of a bare error. */
function isBindRequired(e: unknown): boolean {
  return isIpcError(e) && (e.code === 'NEED_BIND_NEWAPI' || e.code === 'NEED_REBIND_NEWAPI');
}

export default function UsagePage() {
  const [quota, setQuota] = useState<QuotaSummary | null>(null);
  const [usage, setUsage] = useState<UsageDetail | null>(null);
  const [usageMsg, setUsageMsg] = useState<string | null>(null);
  const [timeRange, setTimeRange] = useState<TimeRange>('today');
  const [bindOpen, setBindOpen] = useState(false);
  // 多订阅切换：默认第 0 条（最新的那条），订阅列表变化后回到最新
  const [subIdx, setSubIdx] = useState(0);

  const loadUsage = useCallback((range: TimeRange) => {
    setUsageMsg(null);
    const { start, end } = rangeTimestamps(range);
    return invokeCommand<UsageDetail>('usage.detail', { startTs: start, endTs: end })
      .then(setUsage)
      .catch((e) => {
        if (isBindRequired(e)) setBindOpen(true);
        setUsage(null);
        setUsageMsg(errorMessage(e));
      });
  }, []);

  const refreshQuota = useCallback(() => {
    invokeCommand<QuotaSummary>('usage.getQuota', {})
      .then(setQuota)
      .catch((e) => {
        if (isBindRequired(e)) setBindOpen(true);
        setQuota(null);
      });
  }, []);

  // 绑定成功：关闭弹框并重新拉取余额与明细（等价于原请求重试）
  const onBound = useCallback(() => {
    setBindOpen(false);
    refreshQuota();
    void loadUsage(timeRange);
  }, [refreshQuota, loadUsage, timeRange]);

  useEffect(() => {
    refreshQuota();
    loadUsage(timeRange);
  }, [refreshQuota, loadUsage, timeRange]);

  // 30 分钟自动刷新（不做实时轮询）
  useEffect(() => {
    const t = window.setInterval(() => {
      refreshQuota();
      loadUsage(timeRange);
    }, 30 * 60 * 1000);
    return () => window.clearInterval(t);
  }, [refreshQuota, loadUsage, timeRange]);

  // 手动刷新（时间筛选左侧按钮）
  const [refreshing, setRefreshing] = useState(false);
  const manualRefresh = useCallback(async () => {
    if (refreshing) return;
    setRefreshing(true);
    refreshQuota();
    try { await loadUsage(timeRange); } finally { setRefreshing(false); }
  }, [refreshing, refreshQuota, loadUsage, timeRange]);

  const usd = (v: number | undefined | null, fmt: (n: number) => string) =>
    v === undefined || v === null ? '—' : (
      <>
        {fmt(v)}
        <span className="stat-unit">USD</span>
      </>
    );

  const sub = quota?.subscriptionUsd != null;
  // 多订阅切换卡：subs 已按到期时间倒序（最新在前）
  const subs = quota?.subscriptions ?? [];
  const subCount = subs.length;
  const idx = subCount > 0 ? Math.min(subIdx, subCount - 1) : 0;
  const cur: SubscriptionCard | null = subCount > 0 ? subs[idx] : null;
  const planName = (id: number) =>
    id === 2 ? t('usage.subPlanTrial')
      : id === 3 ? t('usage.subPlanWeek')
        : id === 4 ? t('usage.subPlanMonth')
          : id === 5 ? t('usage.subPlanDay')
            : t('usage.subPlanFallback');
  const subNavStyle: CSSProperties = {
    position: 'absolute', top: '50%', transform: 'translateY(-50%)',
    width: 22, height: 22, borderRadius: '50%', padding: 0, fontSize: 14, lineHeight: 1,
    display: 'flex', alignItems: 'center', justifyContent: 'center',
    border: '1px solid var(--border, #E5E7EB)', background: 'var(--card, #FFF)',
    color: 'var(--text-2, #4B5563)', cursor: 'pointer',
  };
  const okCount = usage?.totalRequests ?? 0;
  const failCount = usage?.errorCount ?? 0;
  const totalCalls = okCount + failCount;
  const rate = totalCalls > 0 ? `${Math.round((okCount / totalCalls) * 100)}%` : '—';

  // 订阅列表变化（新增或过期）时回到最新一条
  useEffect(() => { setSubIdx(0); }, [subCount]);

  return (
    <div>
      <h1>{t('nav.usage')}</h1>
      <div className="sub">{t('config.usageSubtitle')}</div>

      <div className="usage-grid usage-grid--4">
        <div className="stat-card" style={{ position: 'relative' }}>
          <div className="stat-card__label">
            {t('usage.subscriptionBalance')}
            {subCount > 1 && (
              <span style={{ marginLeft: 6, fontSize: 11, fontWeight: 500, opacity: 0.6 }}>{idx + 1}/{subCount}</span>
            )}
          </div>
          <div className="stat-card__value">
            {cur ? usd(cur.remainingUsd, fmtUsdFloor2) : usd(quota?.subscriptionUsd, fmtUsdFloor2)}
          </div>
          <div className="stat-card__sub">
            {cur ? (
              <>
                <div>{planName(cur.planId)} · {t('usage.subQuotaRow')} {fmtUsdFloor2(cur.totalUsd)} · {t('usage.subUsedRow')} {fmtUsdFloor2(cur.usedUsd)}</div>
                <div>{t('usage.subExpireRow')} {cur.endTime > 0 ? formatDateTime(cur.endTime) : '—'}</div>
              </>
            ) : sub ? (
              <>
                <div>{t('usage.subQuotaRow')} {fmtUsdFloor2(quota?.subscriptionTotalUsd ?? 0)} · {t('usage.subUsedRow')} {fmtUsdFloor2(quota?.subscriptionUsedUsd ?? 0)}</div>
                <div>{t('usage.subExpireRow')} {quota?.subscriptionEndTime ? formatDateTime(quota.subscriptionEndTime) : '—'}</div>
              </>
            ) : (
              <div>{t('usage.noSubscription')}</div>
            )}
          </div>
          {subCount > 1 && (
            <>
              <button
                type="button"
                aria-label={t('usage.subSwitchPrev')}
                onClick={() => setSubIdx(Math.max(0, idx - 1))}
                disabled={idx <= 0}
                style={{ ...subNavStyle, left: 4, opacity: idx <= 0 ? 0.3 : 1 }}
              >
                ‹
              </button>
              <button
                type="button"
                aria-label={t('usage.subSwitchNext')}
                onClick={() => setSubIdx(Math.min(subCount - 1, idx + 1))}
                disabled={idx >= subCount - 1}
                style={{ ...subNavStyle, right: 4, opacity: idx >= subCount - 1 ? 0.3 : 1 }}
              >
                ›
              </button>
            </>
          )}
        </div>

        <div className="stat-card">
          <div className="stat-card__label">{t('usage.accountBalance')}</div>
          <div className="stat-card__value">{usd(quota?.remainingUsd, fmtUsdFloor2)}</div>
          <div className="stat-card__sub">
            <div>{t('usage.subPriorityLine1')}</div>
            <div>{t('usage.subPriorityLine2')}</div>
          </div>
        </div>

        <div className="stat-card">
          <div className="stat-card__label">{t('usage.creditsTitle')}</div>
          <div className="stat-card__value">
            {sub ? fmtCredits(quota?.subscriptionUsd ?? 0) : '—'}<span className="stat-unit">{t('config.statCreditsUnit')}</span>
          </div>
          <div className="stat-card__sub">
            <div>{t('usage.creditsOfSubNote')}</div>
            <div>{t('usage.creditsRateNote')}</div>
          </div>
        </div>

        <div className="stat-card">
          <div className="stat-card__label">{t('usage.callsTitle')}</div>
          <div className="stat-card__value">{usage ? okCount.toLocaleString() : '—'}<span className="stat-unit">{t('config.times')}</span></div>
          <div className="stat-card__sub">
            <div>{t('usage.callsOkRow')} {okCount.toLocaleString()} · {t('usage.callsFailRow')} {failCount.toLocaleString()}</div>
            <div>{t('usage.callsRateRow')} {rate} · {rangeLabel(timeRange)}</div>
          </div>
        </div>
      </div>

      <UsageDetailSection range={timeRange} onRange={setTimeRange} usage={usage} usageMsg={usageMsg} onRefresh={manualRefresh} refreshing={refreshing} />

      <BindNewapiDialog open={bindOpen} onClose={() => setBindOpen(false)} onBound={onBound} />
    </div>
  );
}
