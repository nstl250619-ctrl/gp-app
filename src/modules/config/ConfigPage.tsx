// 一键配置页（§八 设计稿同款）：IDE 平台卡 + 定位 + 模型勾选 + 对照表 + 导入。用量已拆到 UsagePage。
import { useCallback, useEffect, useRef, useState } from 'react';
import { Check, Copy, Eye, EyeOff, Wand2, X } from 'lucide-react';
import { errorMessage, invokeCommand, isIpcError } from '@/services/ipc';
import type { ApplyResult, AvailableModels, DetectResult, InstallPlan, KeyItem } from '@/types/ipc';
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
  const [msg, setMsg] = useState<string | null>(null);
  const [result, setResult] = useState<{ ok: boolean; text: string } | null>(null);
  const [busy, setBusy] = useState(false);
  // 「测试连通」：用户主动触发的模型探测（导入不再做预检）
  const [testing, setTesting] = useState(false);
  const [testResults, setTestResults] = useState<Record<string, { ok: boolean; error?: string }>>({});
  const [pathDraft, setPathDraft] = useState('');
  const pathDraftRef = useRef('');
  const pathDirty = useRef(false);
  const [available, setAvailable] = useState<string[]>([]);
  const [modelsOffline, setModelsOffline] = useState(false);
  // 冷启动时序防抖：会话静默重登期间首次拉取可能失败，仅自动重试一次
  const modelsRetryRef = useRef(false);
  const [selected, setSelected] = useState<string[]>([]);
  const selectedRef = useRef<string[]>([]);
  const modelsDirty = useRef(false);
  const [baseUrl, setBaseUrl] = useState(() => loadBaseUrl());
  const baseUrlRef = useRef(loadBaseUrl());
  // 密钥选择（account_list_keys/account_select_key）+ 显示开关
  const [keys, setKeys] = useState<KeyItem[]>([]);
  const [showKey, setShowKey] = useState(false);

  const isSingle = SINGLE_MODEL.has(tab);
  // DSH 专属分组：DSH 卡锁定使用专属 key（不可切换），其他卡用通用 key。
  const isDsh = tab === 'dsh';
  const dshKey = keys.find((k) => k.name === 'DSH-Only');

  const loadPlan = useCallback((ids: string[]) => {
    if (ids.length === 0) {
      setPlan(null);
      setPlanState('ready');
      return;
    }
    setPlanState('loading');
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
        setPlanState('error');
      });
  }, [tab]);

  // keys 列表刷新：跟随每次 load 拉取（会话失效/改分组后自动重试），不再只挂载拉一次
  const refreshKeys = useCallback(async (): Promise<KeyItem[]> => {
    try {
      const ks = await invokeCommand<KeyItem[]>('account.list_keys', {});
      setKeys(ks);
      return ks;
    } catch {
      return [];
    }
  }, []);

  const load = useCallback(() => {
    // DSH 卡兜底：key 列表缺 DSH-Only 时自动创建专属 key（幂等），成功后重算 plan 再刷新。
    void refreshKeys().then((ks) => {
      if (tab === 'dsh' && !ks.some((k) => k.name === 'DSH-Only')) {
        invokeCommand('account.setupDshKey', {})
          .then(() => {
            // V3：专属 key 就绪后必须重算 plan，否则 plan 停留在用通用 key 算出的旧值。
            loadPlan(selectedRef.current);
            return refreshKeys();
          })
          .catch(() => setMsg(t('config.dshKeySetupFailed')));
      }
    });
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
    invokeCommand<AvailableModels>('install.availableModels', { target: tab })
      .then((res) => {
        const list = res.models;
        setModelsOffline(res.offline);
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
            invokeCommand<AvailableModels>('install.availableModels', { target: tab })
              .then((res) => {
                const list = res.models;
                setModelsOffline(res.offline);
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
        } else if (!modelsRetryRef.current) {
          // 冷启动时序：会话静默重登期间首次拉取可能失败（「网关未连通」+ 误导的重新登录提示），
          // 延迟重试一次；仍失败则停留当前态，由用户切卡/操作触发再拉。
          modelsRetryRef.current = true;
          setMsg(null);
          setTimeout(() => load(), 3000);
        }
      });
  }, [tab, loadPlan, refreshKeys]);

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
        // 导入不做任何预检：只把勾选的模型写入配置（预检已拆到「测试连通」）
        target: tab, modelIds: selectedRef.current, verify: false, customPath: pathDraftRef.current || undefined, baseUrl: baseUrlRef.current,
      });
      const ok = r.status === 'applied';
      setResult({ ok, text: ok ? (r.message || t('config.applySuccess')) : r.message });
      pathDirty.current = false;
      modelsDirty.current = false;
      load();
    } catch (e) {
      setResult({ ok: false, text: errorMessage(e) });
    } finally {
      setBusy(false);
    }
  };

  // 「测试连通」：对勾选模型逐个真实测试，不写配置。
  // 通过的标 ✓200；失败的标 × 并自动取消勾选（用户仍可手动重勾，导入时不拦）。
  const testModels = async () => {
    const ids = selectedRef.current;
    if (ids.length === 0) return;
    setTesting(true);
    setResult(null);
    try {
      const r = await invokeCommand<Array<{ model: string; ok: boolean; error?: string | null }>>(
        'install.testModels',
        { target: tab, modelIds: ids, baseUrl: baseUrlRef.current },
      );
      const map: Record<string, { ok: boolean; error?: string }> = {};
      r.forEach((x) => { map[x.model] = { ok: x.ok, error: x.error || undefined }; });
      setTestResults(map);
      const failedSet = new Set(r.filter((x) => !x.ok).map((x) => x.model));
      const okCount = r.filter((x) => x.ok).length;
      if (failedSet.size > 0) {
        // 失败的自动取消勾选（用户可手动重勾）
        const next = selectedRef.current.filter((m) => !failedSet.has(m));
        selectedRef.current = next;
        setSelected(next);
        saveModels(tab, next);
        setResult({
          ok: false,
          text: `${t('config.testDone')}：${okCount} ${t('config.testPassed')}，${failedSet.size} ${t('config.testFailed')}（${t('config.testUnchecked')}）`,
        });
      } else {
        setResult({ ok: true, text: `${t('config.testDone')}：${okCount} ${t('config.testPassed')}` });
      }
    } catch (e) {
      setResult({ ok: false, text: errorMessage(e) });
    } finally {
      setTesting(false);
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
    // 切卡清空测试结果（不同卡的模型集不同，避免旧标记串卡）
    setTestResults({});
    pathDirty.current = false;
    modelsDirty.current = false;
    // 切卡后清空选择，load()（依赖 tab 变化）会自动重拉该卡的模型列表并恢复已保存选择。
    // DSH 卡拉 DSH-Only 分组模型，其他卡拉账号级模型（C4）。
    selectedRef.current = [];
    setSelected([]);
    setPlan(null);
    setPlanState('ready');
    setMsg(!WRITABLE.has(key) ? t('config.tabUnavailable') : null);
  };

  const changeUrl = (u: string) => {
    baseUrlRef.current = u;
    setBaseUrl(u);
    saveBaseUrl(u);
    loadPlan(selectedRef.current);
  };

  const maskKey = (k: string) => (k.length > 12 ? `${k.slice(0, 4)}••••••${k.slice(-4)}` : '••••••');

  const copyKey = async () => {
    if (!plan?.keyAfter) return;
    await navigator.clipboard.writeText(plan.keyAfter);
  };

  const pickKey = async (tokenId: number) => {
    try {
      await invokeCommand('account.select_key', { tokenId });
      const ks = await invokeCommand<KeyItem[]>('account.list_keys', {});
      setKeys(ks);
      setShowKey(false);
      loadPlan(selectedRef.current); // 当前 key 已切换，重新计算 plan
    } catch (e) {
      setMsg(errorMessage(e));
    }
  };

  return (
    <div>
      <h1>{t('nav.config')}</h1>
      <div className="sub">{t('config.subtitle')}</div>

      <IdeTabs tab={tab} detectAll={detectAll} onPick={pickTab} />

      {WRITABLE.has(tab) && (
        <>
        {/* 定位配置文件：裸卡片，无标题 */}
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
        </div>

        {/* OpenAI 兼容模式配置：h2 标题 + 卡片 */}
        <h2 className="section-title">{t('config.willChange')}</h2>
        <div className="card">
          <div className="config-path-row">
            <span className="tag-pill" style={{ width: 76, justifyContent: 'center' }}>{t('config.rowService')}</span>
            <select className="input" style={{ flex: 1 }} value={baseUrl} onChange={(e) => changeUrl(e.target.value)}>
              <option value="https://api.greenpool.cn">https://api.greenpool.cn</option>
              <option value="https://aigw.greenpool.cn">https://aigw.greenpool.cn</option>
            </select>
          </div>

          <div className="config-path-row" style={{ marginTop: 'var(--space-3)' }}>
            <span className="tag-pill" style={{ width: 76, justifyContent: 'center' }}>{t('config.rowKey')}</span>
            <select
              className="input"
              style={{ flex: 1 }}
              value={isDsh ? (dshKey?.tokenId ?? '') : (keys.find((k) => k.selected)?.tokenId ?? '')}
              disabled={isDsh}
              title={isDsh ? t('config.dshKeyLocked') : undefined}
              onChange={(e) => { const id = Number(e.target.value); if (id) void pickKey(id); }}
            >
              {isDsh ? (
                dshKey ? (
                  <option value={dshKey.tokenId}>{showKey && plan ? plan.keyAfter : dshKey.name}</option>
                ) : (
                  <option value="">—</option>
                )
              ) : (
                <>
                  {keys.length === 0 && (
                    <option value="">{plan ? (showKey ? plan.keyAfter : maskKey(plan.keyAfter)) : '—'}</option>
                  )}
                  {keys.map((k) => (
                    <option key={k.tokenId} value={k.tokenId}>
                      {k.selected && showKey && plan ? plan.keyAfter : k.name}
                    </option>
                  ))}
                </>
              )}
            </select>
            <button type="button" className="icon-btn" title={showKey ? t('config.keyHide') : t('config.keyShow')} onClick={() => setShowKey((v) => !v)}>
              {showKey ? <EyeOff size={14} /> : <Eye size={14} />}
            </button>
            <button type="button" className="icon-btn" title={t('common.copy')} onClick={() => void copyKey()}>
              <Copy size={14} />
            </button>
          </div>
        </div>

        {/* 选择要导入的模型：h2 标题 + 卡片 */}
        <h2 className="section-title">{t('config.selectModels')}</h2>
        <div className="card">
          {modelsOffline && <div className="hint" style={{ marginBottom: 'var(--space-2)' }}>{t('configExtra.modelsOffline')}</div>}

          <ModelPicker
            available={available}
            selected={selected}
            single={isSingle}
            showTitle={false}
            testResults={testResults}
            onToggle={toggleModel}
            onSelectAll={selectAllModels}
            onClear={clearModels}
          />

          {planState === 'loading' && (
            <p className="muted" style={{ marginTop: 'var(--space-3)' }}>{t('config.planLoading')}</p>
          )}
          {planState === 'keyMissing' && (
            <p className="muted" style={{ marginTop: 'var(--space-3)' }}>{t('config.needKeyFirst')}</p>
          )}

          <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-4)', marginTop: 'var(--space-4)' }}>
            <button type="button" className="btn btn-grad" disabled={busy || !pathDraft.trim() || selected.length === 0} onClick={apply}>
              <Wand2 size={15} />
              {busy ? t('config.importing') : `${t('config.importModels')} · ${selected.length}`}
            </button>
            <button type="button" className="btn btn--ghost" disabled={testing || busy || selected.length === 0} onClick={testModels}>
              {testing ? t('config.testingModels') : t('config.testModels')}
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
        </>
      )}
    </div>
  );
}
