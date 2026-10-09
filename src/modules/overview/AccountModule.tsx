// 账号卡：只管账号（登录/注册/找回 + 三端联通状态 + 老账号绑定商城），密钥在 KeyModule。
import { useCallback, useEffect, useRef, useState } from 'react';
import { User } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { AppStatus, LinkStatus, WalletSummary } from '@/types/ipc';
import { t } from '@/i18n';
import AccountForms from './AccountForms';

type Mode = 'login' | 'register' | 'reset';

interface Props {
  account: AppStatus | null;
  onRefresh: () => void;
}

export default function AccountModule({ account, onRefresh }: Props) {
  const [mode, setMode] = useState<Mode>('login');
  const [link, setLink] = useState<LinkStatus | null>(null);
  const [bindOpen, setBindOpen] = useState(false);
  const [bindCode, setBindCode] = useState('');
  const [bindShopPwd, setBindShopPwd] = useState('');
  const [bindBusy, setBindBusy] = useState(false);
  const [bindMsg, setBindMsg] = useState<string | null>(null);
  const bindTimer = useRef<number | null>(null);
  const [bindCountdown, setBindCountdown] = useState(0);

  const loadLink = useCallback(() => {
    invokeCommand<LinkStatus>('account.linkStatus', {}).then(setLink).catch(() => {});
  }, []);

  useEffect(() => {
    if (account?.loggedIn) loadLink();
  }, [account?.loggedIn, account?.username, loadLink]);

  useEffect(() => {
    return () => {
      if (bindTimer.current) window.clearInterval(bindTimer.current);
    };
  }, []);

  const startBindCountdown = () => {
    setBindCountdown(30);
    bindTimer.current = window.setInterval(() => {
      setBindCountdown((c) => {
        if (c <= 1 && bindTimer.current) {
          window.clearInterval(bindTimer.current);
          bindTimer.current = null;
          return 0;
        }
        return c - 1;
      });
    }, 1000);
  };

  const sendBindCode = async () => {
    setBindMsg(null);
    try {
      await invokeCommand('shop.sendBindCode', {});
      startBindCountdown();
    } catch (e) {
      setBindMsg(errorMessage(e));
    }
  };

  const bind = async () => {
    setBindBusy(true);
    setBindMsg(null);
    try {
      const args: Record<string, unknown> = { code: bindCode };
      if (bindShopPwd) args.shopPassword = bindShopPwd;
      await invokeCommand<WalletSummary>('shop.bind', args);
      setBindShopPwd('');
      setBindCode('');
      setBindMsg(t('overview.bindDone'));
      setBindOpen(false);
      loadLink();
      onRefresh();
    } catch (e) {
      setBindMsg(errorMessage(e));
    } finally {
      setBindBusy(false);
    }
  };

  const logout = async () => {
    await invokeCommand('account.logout', {});
    setLink(null);
    setMode('login');
    setBindOpen(false);
    onRefresh();
  };

  const stateOk = !!account?.loggedIn && !!link?.newapiOk;
  const shopLinked = !!link?.shopLinked;
  const linkLoading = !!account?.loggedIn && link === null;

  const linkRow = (label: string, ok: boolean | null, okText: string, badText: string) => (
    <div className="record-row">
      <span className="muted">{label}</span>
      <span className="record-row__meta">
        <span className={`dot ${ok === true ? 'dot--ok' : ok === false ? 'dot--warn' : ''}`} />
        <span>{ok === true ? okText : ok === false ? badText : t('common.loading')}</span>
      </span>
    </div>
  );

  return (
    <div className="card">
      <div className="card__head">
        <span className="card__label">
          <User size={16} color="var(--muted)" />
          {t('overview.account')}
        </span>
        <span className="card__state">
          <span className={`dot ${stateOk ? 'dot--ok' : linkLoading ? '' : 'dot--warn'}`} />
          {linkLoading ? t('common.loading') : stateOk ? t('overview.ready') : t('overview.notStarted')}
        </span>
      </div>

      {account?.loggedIn ? (
        <>
          <div className="card__big">{link?.newapiOk ? link.newapiUsername : account.username}</div>

          {linkRow(t('overview.linkLocal'), link ? true : null, t('overview.linkOk'), t('overview.linkNotLinked'))}
          {linkRow(
            t('overview.linkNewapi'),
            link ? !!link.newapiOk : null,
            link?.newapiOk ? `${t('overview.linkOk')} · ${link.newapiUsername}` : t('overview.linkNotLinked'),
            link?.newapiOk === false ? t('overview.linkExpired') : t('overview.linkNotLinked'),
          )}
          <div className="record-row">
            <span className="muted">{t('overview.linkShop')}</span>
            <span className="record-row__meta">
              <span className={`dot ${shopLinked ? 'dot--ok' : 'dot--warn'}`} />
              <span>
                {shopLinked
                  ? t('overview.linkOk')
                  : link?.shopReady
                    ? t('overview.linkNotLinked')
                    : t('overview.linkShopPending')}
              </span>
              {!shopLinked && link?.shopReady && (
                <button type="button" className="link-btn" style={{ marginLeft: 'var(--space-2)' }} onClick={() => setBindOpen(true)}>
                  {t('overview.bindShop')}
                </button>
              )}
            </span>
          </div>

          {!shopLinked && link?.shopError && (
            <div className="error-text" style={{ marginTop: 'var(--space-2)' }}>
              {t('overview.linkShopFail')}：{link.shopError}
            </div>
          )}

          {bindOpen && !shopLinked && link?.shopReady && (
            <div style={{ borderTop: '1px solid var(--border)', marginTop: 'var(--space-2)', paddingTop: 'var(--space-2)' }}>
              <div className="hint" style={{ margin: '0 0 var(--space-2)' }}>{t('overview.bindNote')}</div>
              <div className="input-row">
                <input
                  className="input"
                  style={{ flex: 1 }}
                  placeholder={t('overview.bindCodeLabel')}
                  value={bindCode}
                  onChange={(e) => setBindCode(e.target.value)}
                />
                <button type="button" className="btn" disabled={bindCountdown > 0} onClick={sendBindCode}>
                  {bindCountdown > 0 ? `${bindCountdown}s` : t('overview.sendBindCode')}
                </button>
              </div>
              <div className="field">
                <label className="field__label">
                  {t('overview.bindPasswordLabel')}
                  <span className="muted small"> · {t('overview.bindPasswordHint')}</span>
                </label>
                <input
                  className="input"
                  type="password"
                  value={bindShopPwd}
                  onChange={(e) => setBindShopPwd(e.target.value)}
                />
              </div>
              {bindMsg && <div className="error-text" style={{ marginTop: 'var(--space-2)' }}>{bindMsg}</div>}
              <button
                type="button"
                className="btn btn-primary"
                style={{ marginTop: 'var(--space-2)' }}
                disabled={bindBusy || !bindCode}
                onClick={bind}
              >
                {t('overview.bindBtn')}
              </button>
            </div>
          )}

          <div className="card__foot">
            <button type="button" className="link-btn" onClick={logout}>
              {t('settings.logout')}
            </button>
          </div>
        </>
      ) : (
        <AccountForms mode={mode} onSwitchMode={setMode} onRefresh={onRefresh} />
      )}
    </div>
  );
}

