// 用量分区（统计/明细双 tab）：统计 = 积分按天柱状图；明细 = 分页表格（20 条/页，后端最多 100 条）。
// 时间筛选同时驱动统计卡与两个视图。
import { useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import type { DayStat, UsageDetail as UsageDetailData } from '@/types/ipc';
import { t } from '@/i18n';
import { formatUnix } from '@/utils/time';
import { fmtCreditsSpent, fmtUsd } from '@/utils/fmt';

export type TimeRange = 'today' | '7d' | '30d';

export const RANGES: { key: TimeRange; label: string; days: number }[] = [
  { key: 'today', label: t('config.timeToday'), days: 0 },
  { key: '7d', label: t('config.time7d'), days: 7 },
  { key: '30d', label: t('config.time30d'), days: 30 },
];

export function rangeTimestamps(range: TimeRange): { start: number; end: number } {
  const now = Math.floor(Date.now() / 1000);
  if (range === 'today') {
    const d = new Date(); d.setHours(0, 0, 0, 0);
    return { start: Math.floor(d.getTime() / 1000), end: now };
  }
  const days = range === '7d' ? 7 : 30;
  return { start: now - days * 86400, end: now };
}

export function rangeLabel(range: TimeRange): string {
  return RANGES.find((r) => r.key === range)?.label ?? '';
}

function fmtTokens(p: number, c: number): string {
  return `${(p ?? 0).toLocaleString()} / ${(c ?? 0).toLocaleString()}`;
}

// 与 new-api 计量口径一致：quota/500000 = USD
function quotaUsd(quota: number): number {
  return quota / 500000;
}

const PAGE_SIZE = 20;

type View = 'detail' | 'stats';

/** Full timeline (1/7/30 days), zero-filled. Key date must use UTC+8 (same as backend aggregation). */
function buildTimeline(range: TimeRange, daily: DayStat[]): DayStat[] {
  const map = new Map(daily.map((d) => [d.day, d.credits]));
  const n = range === 'today' ? 1 : range === '7d' ? 7 : 30;
  // UTC+8 day arithmetic: shift epoch, then read with UTC getters (system timezone independent)
  const DAY = 24 * 3600 * 1000;
  const nowUtc8 = Date.now() + 8 * 3600 * 1000;
  const out: DayStat[] = [];
  for (let i = n - 1; i >= 0; i--) {
    const d = new Date(nowUtc8 - i * DAY);
    const key = `${d.getUTCMonth() + 1}/${d.getUTCDate()}`;
    out.push({ day: key, credits: map.get(key) ?? 0 });
  }
  return out;
}

/** Credits-per-day bar chart (full timeline, pure CSS) */
function StatsView({ daily, range, err }: { daily: DayStat[]; range: TimeRange; err: string | null }) {
  const days = buildTimeline(range, daily);
  const max = Math.max(...days.map((d) => d.credits), 0.1);
  const compact = days.length > 10;
  const total = days.reduce((s, d) => s + d.credits, 0);
  if (total <= 0) {
    // 加载失败与真实空数据区分展示（避免"暂无数据"掩盖错误）
    return <p className="muted" style={{ padding: 'var(--space-4)' }}>{err || t('usage.statsEmpty')}</p>;
  }
  return (
    <div style={{ padding: '28px var(--space-4) var(--space-4)' }}>
      <div className="stats-chart">
        {days.map((d) => (
          <div className="stats-chart__col" key={d.day} style={compact ? { minWidth: 20 } : undefined}>
            <span className="stats-chart__val">{d.credits > 0 ? d.credits.toLocaleString() : ''}</span>
            <div className="stats-chart__track">
              <div
                className={d.credits > 0 ? 'stats-chart__bar' : 'stats-chart__bar stats-chart__bar--zero'}
                style={{ height: d.credits > 0 ? `${Math.max(3, (d.credits / max) * 100)}%` : '2px' }}
              />
            </div>
            <span className="stats-chart__label">{compact && d.day.includes('/') ? d.day.split('/')[1] : d.day}</span>
          </div>
        ))}
      </div>
      <div className="hint" style={{ marginTop: 'var(--space-3)' }}>{t('usage.statsUnit')} · {t('usage.subPriorityNote')}</div>
    </div>
  );
}

interface Props {
  range: TimeRange;
  onRange: (r: TimeRange) => void;
  usage: UsageDetailData | null;
  usageMsg: string | null;
  onRefresh: () => void;
  refreshing: boolean;
}

export default function UsageDetailSection({ range, onRange, usage, usageMsg, onRefresh, refreshing }: Props) {
  const [view, setView] = useState<View>('detail');
  const [page, setPage] = useState(1);
  // 数据切换（时间范围/重新拉取）时回到第一页
  useEffect(() => setPage(1), [usage, range]);

  const items = usage?.items ?? [];
  const totalPages = Math.max(1, Math.ceil(items.length / PAGE_SIZE));
  const cur = Math.min(page, totalPages);
  const pageItems = items.slice((cur - 1) * PAGE_SIZE, cur * PAGE_SIZE);

  return (
    <>
      {/* heading row: detail/stats tabs + time filter */}
      <div
        style={{
          display: 'flex',
          alignItems: 'center',
          justifyContent: 'space-between',
          flexWrap: 'wrap',
          gap: 'var(--space-2)',
          marginTop: 'var(--space-6)',
        }}
      >
        <div className="tabs" style={{ marginBottom: 0 }}>
          <button
            type="button"
            className={view === 'detail' ? 'tab tab--active' : 'tab'}
            onClick={() => setView('detail')}
          >
            {t('usage.tabDetail')}
          </button>
          <button
            type="button"
            className={view === 'stats' ? 'tab tab--active' : 'tab'}
            onClick={() => setView('stats')}
          >
            {t('usage.tabStats')}
          </button>
        </div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
          <button
            type="button"
            className="btn btn--ghost"
            disabled={refreshing}
            onClick={onRefresh}
            title={t('common.refresh')}
            style={{ display: 'inline-flex', alignItems: 'center', gap: 5, padding: '5px 12px' }}
          >
            <RefreshCw size={13} className={refreshing ? 'gp-spin' : undefined} />
            {t('common.refresh')}
          </button>
          <div className="tabs" style={{ marginBottom: 0 }}>
          {RANGES.map((r) => (
            <button
              key={r.key}
              type="button"
              className={range === r.key ? 'tab tab--active' : 'tab'}
              onClick={() => onRange(r.key)}
            >
              {r.label}
            </button>
          ))}
          </div>
        </div>
      </div>

      {view === 'stats' ? (
        <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
          <StatsView daily={usage?.daily ?? []} range={range} err={usageMsg} />
        </div>
      ) : (
        <div className="card" style={{ padding: 0, overflow: 'hidden' }}>
          {items.length === 0 && (
            <p className="muted" style={{ padding: 'var(--space-4)' }}>
              {usageMsg || (usage ? t('config.detailEmpty') : t('common.loading'))}
            </p>
          )}
          {items.length > 0 && (
            <>
              <div style={{ overflowX: 'auto' }}>
                <table className="log-table">
                  <thead>
                    <tr>
                      <th>{t('config.colTime')}</th>
                      <th>{t('config.colModel')}</th>
                      <th>{t('config.colTokens')}</th>
                      <th>{t('config.colCost')}</th>
                      <th>{t('config.colDuration')}</th>
                      <th>{t('config.colCredits')}</th>
                    </tr>
                  </thead>
                  <tbody>
                    {pageItems.map((item) => (
                      <tr key={item.id}>
                        <td className="small muted">{formatUnix(item.createdAt)}</td>
                        <td>{item.modelName}</td>
                        <td className="small">{fmtTokens(item.promptTokens, item.completionTokens)}</td>
                        <td className="small">{fmtUsd(quotaUsd(item.quota))}</td>
                        <td className="small">{item.useTime}s</td>
                        <td className="small">{fmtCreditsSpent(quotaUsd(item.quota))}</td>
                      </tr>
                    ))}
                  </tbody>
                </table>
              </div>
              {usage?.truncated && (
                <div className="banner" style={{ margin: 0, borderRadius: 0 }}>{t('config.detailTruncated')}</div>
              )}
              <div className="pagination">
                <button
                  type="button"
                  className="pagination__btn"
                  disabled={cur <= 1}
                  onClick={() => setPage(cur - 1)}
                  title={t('config.pagePrev')}
                >
                  {'‹'}
                </button>
                <span className="small muted">{cur} / {totalPages}</span>
                <button
                  type="button"
                  className="pagination__btn"
                  disabled={cur >= totalPages}
                  onClick={() => setPage(cur + 1)}
                  title={t('config.pageNext')}
                >
                  {'›'}
                </button>
              </div>
            </>
          )}
        </div>
      )}
    </>
  );
}
