// 用量页：4 张等高统计卡（统一三段结构：标签 → 大数字 → 一行紧凑小字）+ 统计/明细分区。
import { useCallback, useEffect, useState } from 'react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { QuotaSummary, UsageDetail } from '@/types/ipc';
import { t } from '@/i18n';
import { fmtCredits, fmtUsdFloor2 } from '@/utils/fmt';
import { formatDateTime } from '@/utils/time';
import UsageDetailSection, { rangeLabel, rangeTimestamps, type TimeRange } from './UsageDetail';

export default function UsagePage() {
  const [quota, setQuota] = useState<QuotaSummary | null>(null);
  const [usage, setUsage] = useState<UsageDetail | null>(null);
  const [usageMsg, setUsageMsg] = useState<string | null>(null);
  const [timeRange, setTimeRange] = useState<TimeRange>('7d');

  const loadUsage = useCallback((range: TimeRange) => {
    setUsageMsg(null);
    const { start, end } = rangeTimestamps(range);
    return invokeCommand<UsageDetail>('usage.detail', { startTs: start, endTs: end })
      .then(setUsage)
      .catch((e) => { setUsage(null); setUsageMsg(errorMessage(e)); });
  }, []);

  const refreshQuota = useCallback(() => {
    invokeCommand<QuotaSummary>('usage.getQuota', {}).then(setQuota).catch(() => setQuota(null));
  }, []);

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
  const okCount = usage?.totalRequests ?? 0;
  const failCount = usage?.errorCount ?? 0;
  const totalCalls = okCount + failCount;
  const rate = totalCalls > 0 ? `${Math.round((okCount / totalCalls) * 100)}%` : '—';

  return (
    <div>
      <h1>{t('nav.usage')}</h1>
      <div className="sub">{t('config.usageSubtitle')}</div>

      <div className="usage-grid usage-grid--4">
        <div className="stat-card">
          <div className="stat-card__label">{t('usage.subscriptionBalance')}</div>
          <div className="stat-card__value">{usd(quota?.subscriptionUsd, fmtUsdFloor2)}</div>
          <div className="stat-card__sub">
            {sub ? (
              <>
                <div>{t('usage.subQuotaRow')} {fmtUsdFloor2(quota?.subscriptionTotalUsd ?? 0)} · {t('usage.subUsedRow')} {fmtUsdFloor2(quota?.subscriptionUsedUsd ?? 0)}</div>
                <div>{t('usage.subExpireRow')} {quota?.subscriptionEndTime ? formatDateTime(quota.subscriptionEndTime) : '—'}</div>
              </>
            ) : (
              <div>{t('usage.noSubscription')}</div>
            )}
          </div>
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
    </div>
  );
}
