// 密钥卡：new-api key 的选择器。只选择、不删除；无 key 时自动生成并默认选中。
import { useCallback, useEffect, useRef, useState } from 'react';
import { KeyRound } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { AppStatus, KeyItem } from '@/types/ipc';
import { t } from '@/i18n';

interface Props {
  account: AppStatus | null;
  onRefresh: () => void;
  /** parent refresh signal: bumped by the header refresh button to re-pull the key list */
  refreshKey: number;
}

export default function KeyModule({ account, onRefresh, refreshKey }: Props) {
  const [keys, setKeys] = useState<KeyItem[] | null>(null);
  const [msg, setMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 自动动作（生成/选中）只尝试一次：失败不重试，避免 effect 因 busy 翻转陷入无限循环
  const autoTried = useRef(false);

  const load = useCallback(() => {
    setMsg(null); // clear stale error before reloading
    invokeCommand<KeyItem[]>('account.listKeys', {})
      .then(setKeys)
      .catch((e) => setMsg(errorMessage(e)));
  }, []);

  useEffect(() => {
    if (!account?.loggedIn) {
      setKeys(null);
      return;
    }
    load();
  }, [account?.loggedIn, refreshKey, load]);

  // 自动规则（只试一次）：无可用 key → 自动生成并选中；有 key 但未选 → 选中第一个启用的
  useEffect(() => {
    if (!keys || busy || autoTried.current) return;
    const enabled = keys.filter((k) => k.status === 1);
    if (enabled.length === 0) {
      autoTried.current = true;
      setBusy(true);
      setMsg(t('overview.keyAutoCreating'));
      invokeCommand('account.setupKey', {})
        .then(() => {
          setMsg(null);
          load();
          onRefresh();
        })
        .catch((e) => setMsg(errorMessage(e)))
        .finally(() => setBusy(false));
      return;
    }
    if (!keys.some((k) => k.selected)) {
      autoTried.current = true;
      select(enabled[0].tokenId);
    }
  }, [keys, busy, load, onRefresh]);

  const select = async (tokenId: number) => {
    setBusy(true);
    setMsg(null);
    try {
      await invokeCommand('account.selectKey', { tokenId });
      load();
      onRefresh();
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const selected = keys?.find((k) => k.selected);
  const ready = !!selected && selected.status === 1;

  return (
    <div className="card">
      <div className="card__head">
        <span className="card__label">
          <KeyRound size={16} color="var(--muted)" />
          {t('overview.apiKey')}
        </span>
        <span className="card__state">
          <span className={`dot ${ready ? 'dot--ok' : ''}`} />
          {ready ? t('overview.ready') : t('overview.notStarted')}
        </span>
      </div>

      {!account?.loggedIn ? (
        <div className="card__big">{t('overview.keyNone')}</div>
      ) : keys === null ? (
        <p className="muted">{t('common.loading')}</p>
      ) : (
        <>
          <div className="field">
            <label className="field__label">{t('overview.keySelect')}</label>
            <select
              className="input"
              value={selected?.tokenId ?? ''}
              disabled={busy}
              onChange={(e) => select(Number(e.target.value))}
            >
              {keys.filter((k) => k.status === 1).length === 0 && (
                <option value="">{t('overview.keyAutoCreating')}</option>
              )}
              {keys.map((k) => (
                <option key={k.tokenId} value={k.tokenId} disabled={k.status !== 1}>
                  {k.name} #{k.tokenId}
                  {k.status !== 1 ? `（${t('overview.keyDisabledTag')}）` : ''}
                  {k.selected ? ` · ${t('overview.keyInUse')}` : ''}
                </option>
              ))}
            </select>
          </div>
          <div className="card__desc">
            {ready ? t('overview.keyReady') : t('overview.keySelectHint')}
          </div>
        </>
      )}

      {msg && (
        <div style={{ marginTop: 'var(--space-2)', display: 'flex', alignItems: 'center', gap: 'var(--space-3)' }}>
          <span className="error-text">{msg}</span>
          <button type="button" className="link-btn" onClick={load}>{t('common.retry')}</button>
        </div>
      )}
    </div>
  );
}
