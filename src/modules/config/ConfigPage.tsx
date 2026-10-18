// 一键配置页（§八 设计稿同款）：IDE 平台卡 + 定位 + 模型勾选 + 对照表 + 导入。用量已拆到 UsagePage。
import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, Copy, Wand2, X } from 'lucide-react';
import { errorMessage, invokeCommand, isIpcError } from '@/services/ipc';
import type { ApplyResult, DetectResult, InstallPlan } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import ModelPicker from './ModelPicker';
import IdeTabs, { type TabKey, WRITABLE, SINGLE_MODEL } from './IdeTabs';

// ---- selection persistence (localStorage, per import target) ----
const BASE_URL_KEY = 'gp.baseUrl';
const BASE_URLS = ['https://api.greenpool.cn', 'https://aigw.greenpool.cn'];
const modelsKey = (t: TabKey) => `gp.models.${t}`;

function loadBaseUrl(): string {
  const saved = localStorage.getItem(BASE_URL_KEY);
  return saved && BASE_URLS.includes(saved) ? saved : BASE_URLS[0];
}

function saveBaseUrl(u: string): void {
  try { localStorage.setItem(BASE_URL_KEY, u); } catch { /* ignore */ }
}

/** restore last saved selection for a target, intersected with the currently available list; fall back to defaults */
function loadSavedModels(t: TabKey, list: string[]): string[] {
  try {
    const arr = JSON.parse(localStorage.getItem(modelsKey(t)) || '[]') as string[];
    const set = new Set(list);
    const kept = arr.filter((m) => set.has(m));
    if (kept.length > 0) return kept;
  } catch { /* ignore */ }
  return SINGLE_MODEL.has(t) ? list.slice(0, 1) : list;
}

function saveModels(t: TabKey, ids: string[]): void {
  try { localStorage.setItem(modelsKey(t), JSON.stringify(ids)); } catch { /* ignore */ }
}

export default function ConfigPage({ onNavigate }: PageProps) {
  const [tab, setTab] = useState<TabKey>('workbuddy');
  const [detect, setDetect] = useState<DetectResult | null>(null);
  const [detectAll, setDetectAll] = useState<DetectResult[]>([]);
  const [plan, setPlan] = useState<InstallPlan | null>(null);
  const [planState, setPlanState] = useState<'loading' | 'ready' | 'keyMissing' | 'error'>('loading');
  const [planError, setPlanError] = useState<string | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  const [pathDraft, setPathDraft] = useState('');
  const pathDraftRef = useRef('');
  const pathDirty = useRef(false);
  const [available, setAvailable] = useState<string[]>([]);
  const [selected, setSelected] = useState<string[]>([]);
  const selectedRef = useRef<string[]>([]);
  const modelsDirty = useRef(false);
  const [baseUrl, setBaseUrl] = useState(() => loadBaseUrl());
  const baseUrlRef = useRef(loadBaseUrl());

  const isSingle = SINGLE_MODEL.has(tab);

  const loadPlan = useCallback((ids: string[]) => {
    if (ids.length === 0) {
      setPlan(null);
      setPlanState('ready');
      return;
    }
    setPlanState('loading');
    setPlanError(null);
    const fetchPlan = () =>
      invokeCommand<InstallPlan>('install.plan', {
        target: tab, modelIds: ids, customPath: pathDraftRef.current || undefined, baseUrl: baseUrlRef.current,
      });
    fetchPlan()
      .then((p) => { setPlan(p); setPlanState('ready'); })
      .catch(async (e) => {
        // Key missing fallback (overview page removed; login auto-setup may have been skipped
        // for already-logged-in sessions): generate a key once, then retry the plan.
        if (isIpcError(e) && e.code === 'KEY_MISSING') {
          try {
            await invokeCommand('account.setupKey', {});
            fetchPlan()
              .then((p) => { setPlan(p); setPlanState('ready'); })
              .catch(() => setPlanState('keyMissing'));
            return;
          } catch {
            setPlanState('keyMissing'); // setup failed: show key-missing hint, no silent swallow
            return;
          }
        }
        setPlan(null);
        setPlanError(errorMessage(e));
        setPlanState('error');
      });
  }, [tab]);

  const load = useCallback(() => {
    invokeCommand<DetectResult[]>('install.detect', {})
      .then((list) => {
        setDetectAll(list);
        const d = list.find((x) => x.target === tab) ?? null;
        setDetect(d);
        if (!pathDirty.current) {
          const p = d?.path ?? '';
          pathDraftRef.current = p;
          setPathDraft(p);
        }
      })
      .catch((e) => setMsg(errorMessage(e)));
    invokeCommand<string[]>('install.availableModels', {})
      .then((list) => {
        setAvailable(list);
        if (!modelsDirty.current) {
          // restore last saved selection for this target (per-target persistence),
          // then persist it immediately so the first-seen state is remembered too.
          const init = loadSavedModels(tab, list);
          saveModels(tab, init);
          selectedRef.current = init;
          setSelected(init);
          loadPlan(init);
        }
      })
      .catch(async (e) => {
        // Key-missing fallback: auto-create the key once, then refetch model list.
        if (isIpcError(e) && e.code === 'KEY_MISSING') {
          try {
            await invokeCommand('account.setupKey', {});
            invokeCommand<string[]>('install.availableModels', {})
              .then((list) => {
                setAvailable(list);
                if (!modelsDirty.current) {
                  const init = loadSavedModels(tab, list);
                  saveModels(tab, init);
                  selectedRef.current = init;
                  setSelected(init);
                  loadPlan(init);
                }
              })
              .catch(() => {});
          } catch {
            setPlanState('keyMissing'); // surface instead of silent swallow
          }
        }
      });
  }, [tab, loadPlan]);

  useEffect(() => { load(); }, [load]);

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
        target: tab, modelIds: selectedRef.current, verify: true, customPath: pathDraftRef.current || undefined, baseUrl: baseUrlRef.current,
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
    // Single-model targets behave as radio: picking one replaces the selection.
    const next = isSingle
      ? [id]
      : selectedRef.current.includes(id)
        ? selectedRef.current.filter((x) => x !== id)
        : [...selectedRef.current, id];
    selectedRef.current = next;
    setSelected(next);
    saveModels(tab, next);
    loadPlan(next);
  };

  const selectAllModels = () => {
    modelsDirty.current = true;
    // "All" on a single-model target means the primary model.
    const next = isSingle ? available.slice(0, 1) : available;
    selectedRef.current = next;
    setSelected(next);
    saveModels(tab, next);
    loadPlan(next);
  };

  const clearModels = () => {
    modelsDirty.current = true;
    selectedRef.current = [];
    setSelected([]);
    saveModels(tab, []);
    setPlan(null);
    setPlanState('ready');
  };

  const pickTab = (key: TabKey) => {
    setTab(key);
    setResult(null);
    pathDirty.current = false;
    modelsDirty.current = false;
    // restore this target's last saved selection immediately (load() will re-apply it once models arrive)
    if (available.length > 0) {
      const init = loadSavedModels(key, available);
      saveModels(key, init);
      selectedRef.current = init;
      setSelected(init);
      loadPlan(init);
    }
    setMsg(!WRITABLE.has(key) ? t('config.tabUnavailable') : null);
  };

  const changeUrl = (u: string) => {
    baseUrlRef.current = u;
    setBaseUrl(u);
    saveBaseUrl(u);
    loadPlan(selectedRef.current);
  };

  const rows = () => [
    { field: t('config.rowService'), value: plan?.baseUrlAfter || '—' },
    { field: t('config.rowKey'), value: plan ? `${plan.keyAfter}${t('config.keyPlain')}` : '—' },
  ];

  return (
    <div>
      <h1>{t('nav.config')}</h1>
      <div className="sub">{t('config.subtitle')}</div>

      <IdeTabs tab={tab} detectAll={detectAll} onPick={pickTab} />

      {WRITABLE.has(tab) && (
        <div className="card">
          <div className="config-path-row">
            <span className={detect?.detected ? 'tag-pill' : 'tag-pill tag-pill--dim'}>
              <span className="tag-pill__dot" />
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

          <div className="config-path-row" style={{ marginTop: 'var(--space-4)' }}>
            <span className="muted small" style={{ whiteSpace: 'nowrap' }}>{t('config.rowService')}</span>
            <select className="input" style={{ flex: 1 }} value={baseUrl} onChange={(e) => changeUrl(e.target.value)}>
              <option value="https://api.greenpool.cn">https://api.greenpool.cn</option>
              <option value="https://aigw.greenpool.cn">https://aigw.greenpool.cn</option>
            </select>
          </div>
          {baseUrl.includes('aigw') && (
            <div className="hint" style={{ marginTop: 'var(--space-2)' }}>{t('config.aigwVpnTip')}</div>
          )}

          <ModelPicker
            available={available}
            selected={selected}
            single={isSingle}
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
                      <td><span className="diff-table__value">{r.value}</span></td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </>
          ) : planState === 'loading' ? (
            <p className="muted" style={{ marginTop: 'var(--space-3)' }}>{t('config.planLoading')}</p>
          ) : planState === 'keyMissing' ? (
            <p className="muted" style={{ marginTop: 'var(--space-3)' }}>{t('config.needKeyFirst')}</p>
          ) : planState === 'error' ? (
            <p className="muted" style={{ marginTop: 'var(--space-3)', color: 'var(--danger)' }}>{planError}</p>
          ) : null}

          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-4)', marginTop: 'var(--space-4)' }}>
            <button type="button" className="btn btn-primary" disabled={busy || !pathDraft.trim() || selected.length === 0} onClick={apply}>
              <Wand2 size={15} />
              {t('config.importModels')} · {selected.length}
            </button>
            <button type="button" className="btn btn--ghost" onClick={() => onNavigate('records')}>
              {t('config.watchRecords')}
            </button>
          </div>

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
    </div>
  );
}
