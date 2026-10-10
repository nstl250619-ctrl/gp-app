// 配置与用量：压缩配置卡（对照表数据全部来自真实计划）+ 用量统计 5 卡 + 用量明细（UsageDetail 组件）。
import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, Copy, Info, Wand2, X } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type {
  ApplyResult, DetectResult, InstallPlan, InstanceCapability,
  QuotaSummary, UsageDetail,
} from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import { formatRelative } from '@/utils/time';
import { fmtCredits, fmtUsdCeil2, fmtUsdFloor2 } from '@/utils/fmt';
import UsageDetailSection, { rangeLabel, rangeTimestamps, type TimeRange } from './UsageDetail';
import ModelPicker from './ModelPicker';

type TabKey = 'workbuddy' | 'codebuddy' | 'openclaw' | 'hermes';
const TABS: TabKey[] = ['workbuddy', 'codebuddy', 'openclaw', 'hermes'];
const WRITABLE = new Set<TabKey>(['workbuddy', 'codebuddy']);

export default function ConfigPage({ onNavigate }: PageProps) {
  const [tab, setTab] = useState<TabKey>('workbuddy');
  const [detect, setDetect] = useState<DetectResult | null>(null);
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [quota, setQuota] = useState<QuotaSummary | null>(null);
  const [probe, setProbe] = useState<InstanceCapability | null>(null);
  const [probeAt, setProbeAt] = useState<Date>(new Date());
  const [msg, setMsg] = useState<string | null>(null);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // 配置文件路径（用户可编辑：分身/重命名场景）；ref 供 load 稳定读取，避免 useCallback 循环
  const [pathDraft, setPathDraft] = useState('');
  const pathDraftRef = useRef('');
  const pathDirty = useRef(false);
  // 模型勾选：可用列表（账号在 new-api 实际可用）+ 已勾选 id；ref 供 load/apply 稳定读取
  const [available, setAvailable] = useState<string[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const selectedRef = useRef<string[]>([]);
  const modelsDirty = useRef(false);

  const [timeRange, setTimeRange] = useState<TimeRange>('7d');
  const [usage, setUsage] = useState<UsageDetail | null>(null);
  const [usageMsg, setUsageMsg] = useState<string | null>(null);

  const loadPlan = useCallback((ids: string[]) => {
    if (ids.length === 0) {
      setPlan(null);
      return;
    }
    invokeCommand<InstallPlan>('install.plan', {
      target: tab, modelIds: ids, customPath: pathDraftRef.current || undefined,
    })
      .then(setPlan)
      .catch(() => setPlan(null));
  }, [tab]);

  const load = useCallback(() => {
    invokeCommand<DetectResult[]>('install.detect', {})
      .then((list) => {
        const d = list.find((x) => x.target === tab) ?? null;
        setDetect(d);
        // 用户未手动编辑过路径时，输入框跟随自动定位/持久化 override 的值
        if (!pathDirty.current) {
          const p = d?.path ?? '';
          pathDraftRef.current = p;
          setPathDraft(p);
        }
      })
      .catch((e) => setMsg(errorMessage(e)));
    // 拉账号可用模型；未手动改过则默认全选，并按勾选刷新计划
    invokeCommand<string[]>('install.availableModels', {})
      .then((list) => {
        setAvailable(list);
        if (!modelsDirty.current) {
          selectedRef.current = list;
          setSelected(list);
          loadPlan(list);
        }
      })
      .catch(() => {});
    if (!probe) {
      invokeCommand<InstanceCapability>('instance.probe', {})
        .then((p) => { setProbe(p); setProbeAt(new Date()); })
        .catch(() => {});
    }
    invokeCommand<QuotaSummary>('usage.getQuota', {}).then(setQuota).catch(() => setQuota(null));
  }, [tab, loadPlan]);

  const loadUsage = useCallback((range: TimeRange) => {
    setUsageMsg(null);
    const { start, end } = rangeTimestamps(range);
    invokeCommand<UsageDetail>('usage.detail', { startTs: start, endTs: end })
      .then(setUsage)
      .catch((e) => { setUsage(null); setUsageMsg(errorMessage(e)); });
  }, []);

  // 刷新按钮：余额 + 用量明细 + 数据时间全部重拉
  const refreshAll = useCallback(() => {
    setProbeAt(new Date());
    invokeCommand<InstanceCapability>('instance.probe', {})
      .then(setProbe)
      .catch(() => {});
    invokeCommand<QuotaSummary>('usage.getQuota', {}).then(setQuota).catch(() => setQuota(null));
    loadUsage(timeRange);
  }, [loadUsage, timeRange]);

  useEffect(() => { load(); }, [load]);
  useEffect(() => { loadUsage(timeRange); }, [timeRange, loadUsage]);

  const copyPath = async () => {
    const p = pathDraft || plan?.path;
    if (p) await navigator.clipboard.writeText(p);
  };

  const apply = async () => {
    setBusy(true);
    setMsg(null);
    setResult(null);
    try {
      const r = await invokeCommand<ApplyResult>('install.apply', {
        target: tab, modelIds: selectedRef.current, verify: true, customPath: pathDraftRef.current || undefined,
      });
      const ok = r.status === 'applied';
      setResult({ ok, text: ok ? t('config.applySuccess') : r.message });
      pathDirty.current = false;
      modelsDirty.current = false;
      load();
    } catch (e) {
      setResult({ ok: false, text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  const toggleModel = (id: string) => {
    modelsDirty.current = true;
    const next = selectedRef.current.includes(id)
      ? selectedRef.current.filter((x) => x !== id)
      : [...selectedRef.current, id];
    selectedRef.current = next;
    setSelected(next);
    loadPlan(next);
  };

  const selectAllModels = () => {
    modelsDirty.current = true;
    selectedRef.current = available;
    setSelected(available);
    loadPlan(available);
  };

  const clearModels = () => {
    modelsDirty.current = true;
    selectedRef.current = [];
    setSelected([]);
    setPlan(null);
  };

  const pickTab = (key: TabKey) => {
    setTab(key);
    setResult(null);
    pathDirty.current = false;
    modelsDirty.current = false;
    setMsg(!WRITABLE.has(key) ? t('config.tabUnavailable') : null);
  };

  // 对照表（两列）：行名 + 将写入的真实值
  const rows = () => [
    { field: t('config.rowService'), value: plan?.baseUrlAfter || '—' },
    { field: t('config.rowKey'), value: plan?.keyAfter || '—' },
  ];

  return (
    <div>
      <div className="tabs">
        {TABS.map((key) => (
          <button
            key={key}
            type="button"
            className={tab === key ? 'tab tab--active' : 'tab'}
            onClick={() => pickTab(key)}
          >
            {t(`config.tab${key.charAt(0).toUpperCase()}${key.slice(1)}`)}
          </button>
        ))}
      </div>

      {WRITABLE.has(tab) && !!(detect?.path || pathDraft) && (
        <div className="card">
          {/* status badge + editable path + copy in one row */}
          <div className="config-path-row">
            <span className={detect?.detected ? 'status-tag status-tag--ok' : 'status-tag'}>
              <Check size={14} color={detect?.detected ? 'var(--success)' : 'var(--muted)'} />
              {detect?.detected ? t('config.found') : t('config.notLocated')}
            </span>
            <input
              className="input"
              style={{ flex: 1 }}
              value={pathDraft}
              placeholder={t('config.pathPlaceholder')}
              onChange={(e) => { pathDirty.current = true; pathDraftRef.current = e.target.value; setPathDraft(e.target.value); }}
            />
            <button type="button" className="icon-btn" onClick={copyPath} title={t('common.copy')}>
              <Copy size={14} />
            </button>
          </div>
          <div className="hint" style={{ marginTop: 'var(--space-2)' }}>{t('config.pathEditableHint')}</div>

          <ModelPicker
            available={available}
            selected={selected}
            onToggle={toggleModel}
            onSelectAll={selectAllModels}
            onClear={clearModels}
          />

          {plan ? (
            <>
              <div className="section-title" style={{ marginTop: 'var(--space-4)' }}>{t('config.willChange')}</div>
              <table className="diff-table">
                <tbody>
                  {rows().map((r) => (
                    <tr key={r.field}>
                      <td className="diff-table__field">{r.field}</td>
                      <td>
                        <span className="diff-table__after">
                          <span className="muted">→</span>
                          <span className="diff-table__value">{r.value}</span>
                        </span>
                      </td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          ) : (
            <p className="muted" style={{ marginTop: 'var(--space-3)' }}>{t('config.needKeyFirst')}</p>
          )}

          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-4)', marginTop: 'var(--space-4)' }}>
            <button type="button" className="btn btn-primary" disabled={busy || !pathDraft.trim() || selected.length === 0} onClick={apply}>
              <Wand2 size={15} />
              {t('config.importModels')} · {selected.length}
            </button>
            <button type="button" className="link-btn" onClick={() => onNavigate('records')}>
              {t('config.watchRecords')}
            </button>
          </div>

          {/* apply result banner: success green / failure red */}
          {result && (
            <div className={`apply-banner ${result.ok ? 'apply-banner--ok' : 'apply-banner--fail'}`}>
              {result.ok
                ? <Check size={16} color="var(--success)" />
                : <X size={16} color="var(--danger)" />}
              <span>{result.text}</span>
            </div>
          )}
          {msg && <div className="hint" style={{ marginTop: 'var(--space-2)' }}>{msg}</div>}
        </div>
      )}

      {/* ---- usage stats ---- */}
      <div className="section-title" style={{ marginTop: 'var(--space-6)' }}>{t('config.usageTitle')}</div>
      <div className="section-subtitle">{t('config.usageSubtitle')}</div>
      <div className="banner">
        <Info size={14} />
        {t('config.dataFrom')} {formatRelative(probeAt)}
        {probe && !probe.reachable ? `（${t('config.offline')}）` : ''}
      </div>

      {/* time filter lives in UsageDetail heading row; stat cards keep range sub-labels */}
      <div className="usage-grid">
        <div className="stat-card">
          <div className="stat-card__label">{t('config.statBalance')}</div>
          <div className="stat-card__value">{quota ? fmtUsdFloor2(quota.remainingUsd) : '—'}</div>
          {quota && quota.remainingUsd < 0 && (
            <div className="stat-card__sub" style={{ color: 'var(--danger)' }}>{t('wallet.overdraft')}</div>
          )}
        </div>
        <div className="stat-card">
          <div className="stat-card__label">{t('config.statSpent')}</div>
          <div className="stat-card__value">{quota ? fmtUsdCeil2(quota.usedUsd) : '—'}</div>
          <div className="stat-card__sub">{quota?.username}</div>
        </div>
        <div className="stat-card">
          <div className="stat-card__label">{t('config.statCredits')}</div>
          <div className="stat-card__value">{quota ? fmtCredits(quota.remainingUsd) : '—'}</div>
          <div className="stat-card__sub">{t('config.statCreditsBalance')}</div>
        </div>
        <div className="stat-card">
          <div className="stat-card__label">{t('config.statTokens')}</div>
          <div className="stat-card__value">{usage ? usage.totalTokens.toLocaleString() : '—'}</div>
          <div className="stat-card__sub">{usageMsg || rangeLabel(timeRange)}</div>
        </div>
        <div className="stat-card">
          <div className="stat-card__label">{t('config.statCalls')}</div>
          <div className="stat-card__value">{usage ? usage.totalRequests.toLocaleString() : '—'}</div>
          <div className="stat-card__sub">{usage ? `${t('config.times')} · ${quota?.group || ''}` : ''}</div>
        </div>
      </div>

      <UsageDetailSection range={timeRange} onRange={setTimeRange} usage={usage} usageMsg={usageMsg} onRefresh={refreshAll} />
    </div>
  );
}
