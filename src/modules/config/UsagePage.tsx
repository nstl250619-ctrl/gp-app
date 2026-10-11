// 用量页：4 张等高统计卡（统一三段结构：标签 → 大数字 → 一行紧凑小字）+ 统计/明细分区。
import { useCallback, useEffect, useState } from 'react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { QuotaSummary, UsageDetail } from '@/types/ipc';
import { t } from '@/i18n';
import { fmtCredits, fmtUsdFloor2 } from '@/utils/fmt';
import { formatUnix } from '@/utils/time';
import UsageDetailSection, { rangeLabel, rangeTimestamps, type TimeRange } from './UsageDetail';

export default function UsagePage() {
  const [quota, setQuota] = useState<QuotaSummary | null>(null);
  const [usage, setUsage] = useState<UsageDetail | null>(null);
  const [usageMsg, setUsageMsg] = useState<string | null>(null);
  const [timeRange, setTimeRange] = useState<TimeRange>('7d');

  const loadUsage = useCallback((range: TimeRange) => {
    setUsageMsg(null);
    const { start, end } = rangeTimestamps(range);
    invokeCommand<UsageDetail>('usage.detail', { startTs: start, endTs: end })
      .then(setUsage)
      .catch((e) => { setUsage(null); setUsageMsg(errorMessage(e)); });
  }, []);

  useEffect(() => {
    loadUsage(timeRange);
  }, [loadUsage, timeRange]);

  // 余额/用量统一刷新（页面保活下不重新挂载，兑换等外部变更靠事件驱动）
  const refreshQuota = useCallback(() => {
    invokeCommand<QuotaSummary>('usage.getQuota', {}).then(setQuota).catch(() => setQuota(null));
  }, []);

  useEffect(() => {
    refreshQuota();
    window.addEventListener('gp:balance-changed', refreshQuota);
    return () => window.removeEventListener('gp:balance-changed', refreshQuota);
  }, [refreshQuota]);

  // 兑换成功（subscription/quota 变化）同时刷新明细分区
  useEffect(() => {
    const onChange = () => loadUsage(timeRange);
    window.addEventListener('gp:balance-changed', onChange);
    return () => window.removeEventListener('gp:balance-changed', onChange);
  }, [loadUsage, timeRange]);

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
                <div>{t('usage.subExpireRow')} {quota?.subscriptionEndTime ? formatUnix(quota.subscriptionEndTime) : '—'}</div>
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

      <UsageDetailSection range={timeRange} onRange={setTimeRange} usage={usage} usageMsg={usageMsg} />
    </div>
  );
}
