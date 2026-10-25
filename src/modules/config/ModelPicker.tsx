// 模型勾选列表：账号可用模型 + 全选 / 清空 + 已选计数。
// single=true 时为单模型目标（Codex / Claude Code）：radio 式选择、隐藏「全选」、显示单模型提示。
// testResults：用户点「测试连通」后每个模型的探测结果（✓ 200 / × 原因），仅展示不影响勾选。
import { t } from '@/i18n';

interface Props {
  available: string[];
  selected: string[];
  single?: boolean;
  showTitle?: boolean;
  testResults?: Record<string, { ok: boolean; slow?: boolean; error?: string }>;
  onToggle: (id: string) => void;
  onSelectAll: () => void;
  onClear: () => void;
  /** 重新拉取可用模型列表（网关掉线回落离线清单后，一键重连） */
  onRefresh?: () => void;
  refreshing?: boolean;
}

export default function ModelPicker({ available, selected, single = false, showTitle = true, testResults, onToggle, onSelectAll, onClear, onRefresh, refreshing = false }: Props) {
  return (
    <>
      {showTitle && <div className="section-title" style={{ marginTop: 'var(--space-4)' }}>{t('config.selectModels')}</div>}
      <div className="model-picker">
        <div className="model-picker__head">
          {!single && <button type="button" className="link-btn" onClick={onSelectAll}>{t('config.selectAll')}</button>}
          <button type="button" className="link-btn" onClick={onClear}>{t('config.clear')}</button>
          {onRefresh && (
            <button type="button" className="link-btn" onClick={onRefresh} disabled={refreshing}>
              {refreshing ? t('config.refreshing') : t('config.refresh')}
            </button>
          )}
          <span className="muted small" style={{ marginLeft: 'auto' }}>
            {t('config.selectedModels')} {selected.length}
          </span>
        </div>
        {single && <div className="hint" style={{ marginBottom: 'var(--space-2)' }}>{t('config.singleModelHint')}</div>}
        <div className="model-picker__list">
          {available.map((m) => {
            const tr = testResults?.[m];
            return (
              <label key={m} className="model-row">
                <input type={single ? 'radio' : 'checkbox'} name={single ? 'gp-single-model' : undefined} checked={selected.includes(m)} onChange={() => onToggle(m)} />
                <span className="gp-prefix">GP:</span>
                <span>{m}</span>
                {tr && (
                  <span
                    title={tr.error || 'HTTP 200'}
                    style={{
                      marginLeft: 'auto',
                      fontWeight: 600,
                      fontSize: 12,
                      whiteSpace: 'nowrap',
                      color: !tr.ok ? '#dc2626' : tr.slow ? '#d97706' : '#16a34a',
                    }}
                  >
                    {!tr.ok ? '×' : tr.slow ? t('config.modelCrowded') : '✓ 200'}
                  </span>
                )}
              </label>
            );
          })}
        </div>
      </div>
    </>
  );
}
