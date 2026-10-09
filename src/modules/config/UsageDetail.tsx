// 用量明细区（标题行 + 刷新 + 时间筛选 + 分页明细表，200 条/页）。
// 时间筛选同时驱动统计卡的 Token 消耗/调用次数（父组件通过 rangeLabel/rangeTimestamps 复用）。
import { useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import type { UsageDetail as UsageDetailData } from '@/types/ipc';
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

// 与 new-api 计量口径一致：quota/500000 = USD；积分 = USD × 1000（$1=1000 积分）
function quotaUsd(quota: number): number {
  return quota / 500000;
}

const PAGE_SIZE = 200;

interface Props {
  range: TimeRange;
  onRange: (r: TimeRange) => void;
  usage: UsageDetailData | null;
  usageMsg: string | null;
  onRefresh: () => void;
}

export default function UsageDetailSection({ range, onRange, usage, usageMsg, onRefresh }: Props) {
  const [page, setPage] = useState(1);
  // 数据切换（时间范围/重新拉取）时回到第一页
  useEffect(() => setPage(1), [usage]);

  const items = usage?.items ?? [];
  const totalPages = Math.max(1, Math.ceil(items.length / PAGE_SIZE));
  const cur = Math.min(page, totalPages);
  const pageItems = items.slice((cur - 1) * PAGE_SIZE, cur * PAGE_SIZE);

  return (
    <>
      {/* heading row with the time filter on the right */}
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
        <div className="section-title" style={{ margin: 0 }}>{t('config.detailTitle')}</div>
        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-2)' }}>
          <button
            type="button"
            className="link-btn"
            style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }}
            onClick={onRefresh}
          >
            <RefreshCw size={14} />
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
    </>
  );
}
