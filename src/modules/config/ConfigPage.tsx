// 一键配置页（§八 设计稿同款）：IDE 平台卡 + 定位 + 模型勾选 + 对照表 + 导入。用量已拆到 UsagePage。
import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, Copy, Wand2, X } from 'lucide-react';
import { errorMessage, invokeCommand, isIpcError } from '@/services/ipc';
import type { ApplyResult, DetectResult, InstallPlan } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import ModelPicker from './ModelPicker';
import IdeTabs, { type TabKey, WRITABLE } from './IdeTabs';

export default function ConfigPage({ onNavigate }: PageProps) {
  const [tab, setTab] = useState<TabKey>('workbuddy');
  const [detect, setDetect] = useState<DetectResult | null>(null);
  const [detectAll, setDetectAll] = useState<DetectResult[]>([]);
  const [plan, setPlan] = useState<InstallPlan | null>(null);
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
  const [baseUrl, setBaseUrl] = useState('https://api.greenpool.cn');
  const baseUrlRef = useRef('https://api.greenpool.cn');

  const loadPlan = useCallback((ids: string[]) => {
    if (ids.length === 0) {
      setPlan(null);
      return;
    }
    const fetchPlan = () =>
      invokeCommand<InstallPlan>('install.plan', {
        target: tab, modelIds: ids, customPath: pathDraftRef.current || undefined, baseUrl: baseUrlRef.current,
      });
    fetchPlan()
      .then(setPlan)
      .catch(async (e) => {
        // Key missing fallback (overview page removed; login auto-setup may have been skipped
        // for already-logged-in sessions): generate a key once, then retry the plan.
        if (isIpcError(e) && e.code === 'KEY_MISSING') {
          try {
            await invokeCommand('account.setupKey', {});
            fetchPlan().then(setPlan).catch(() => setPlan(null));
            return;
          } catch {
            /* setup failed: fall through to empty plan */
          }
        }
        setPlan(null);
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
          selectedRef.current = list;
          setSelected(list);
          loadPlan(list);
        }
      })
      .catch(async (e) => {
        // 密钥缺失兜底（总览页已移除，登录自动建 key 失败时在此补救）：自动生成后重试一次
        if (isIpcError(e) && e.code === 'KEY_MISSING') {
          try {
            await invokeCommand('account.setupKey', {});
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
          } catch {
            /* setup failed: keep needKeyFirst hint */
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

  const changeUrl = (u: string) => {
    baseUrlRef.current = u;
    setBaseUrl(u);
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
          ) : (
            <p className="muted" style={{ marginTop: 'var(--space-3)' }}>{t('config.needKeyFirst')}</p>
          )}

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
