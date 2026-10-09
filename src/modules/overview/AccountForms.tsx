// 账号表单（登录/注册/找回密码三态）。状态自含，卡片外壳见 AccountModule。
import { useEffect, useRef, useState } from 'react';
import { errorMessage, invokeCommand, isIpcError } from '@/services/ipc';
import type { AccountStatus, ResetPasswordResult } from '@/types/ipc';
import { t } from '@/i18n';
import { isValidEmail } from '@/utils/time';

type Mode = 'login' | 'register' | 'reset';

interface Props {
  mode: Mode;
  onSwitchMode: (m: Mode) => void;
  onRefresh: () => void;
}

function parseResetLink(link: string): { email: string; token: string } | null {
  try {
    const u = new URL(link.trim());
    const email = u.searchParams.get('email') ?? '';
    const token = u.searchParams.get('token') ?? '';
    if (!email || !token) return null;
    return { email, token };
  } catch {
    return null;
  }
}

export default function AccountForms({ mode, onSwitchMode, onRefresh }: Props) {
  const [identifier, setIdentifier] = useState('');
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [password, setPassword] = useState('');
  const [resetLink, setResetLink] = useState('');
  const [countdown, setCountdown] = useState(0);
  const [msg, setMsg] = useState<string | null>(null);
  const [okMsg, setOkMsg] = useState<string | null>(null);
  const [newPwd, setNewPwd] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    return () => {
      if (timerRef.current) window.clearInterval(timerRef.current);
    };
  }, []);

  const startCountdown = () => {
    setCountdown(30);
    timerRef.current = window.setInterval(() => {
      setCountdown((c) => {
        if (c <= 1 && timerRef.current) {
          window.clearInterval(timerRef.current);
          timerRef.current = null;
          return 0;
        }
        return c - 1;
      });
    }, 1000);
  };

  const emailOk = isValidEmail(email);
  const resetParsed = parseResetLink(resetLink);

  const login = async () => {
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      await invokeCommand<AccountStatus>('account.login', { username: identifier, password });
      setPassword('');
      onRefresh();
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const sendCode = async () => {
    if (sending) return;
    setSending(true);
    setMsg(null);
    setOkMsg(null);
    try {
      await invokeCommand('account.sendCode', { email });
      setOkMsg(t('settings.codeSent'));
      startCountdown();
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setSending(false);
    }
  };

  const register = async () => {
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      await invokeCommand<AccountStatus>('account.register', { email, password, code });
      setPassword('');
      setCode('');
      setOkMsg(t('settings.registerDone'));
      onRefresh();
    } catch (e) {
      // 邮箱已注册：带邮箱切到登录页，别让人反复发码
      if (isIpcError(e) && e.code === 'EMAIL_TAKEN') {
        setIdentifier(email);
        setPassword('');
        setMsg(errorMessage(e));
        onSwitchMode('login');
      } else {
        setMsg(errorMessage(e));
      }
    } finally {
      setBusy(false);
    }
  };

  const sendResetMail = async () => {
    setMsg(null);
    setOkMsg(null);
    try {
      await invokeCommand('account.sendPasswordReset', { email });
      setOkMsg(t('settings.resetSent'));
    } catch (e) {
      setMsg(errorMessage(e));
    }
  };

  const reset = async () => {
    if (!resetParsed) {
      setMsg(t('settings.resetLinkInvalid'));
      return;
    }
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      const r = await invokeCommand<ResetPasswordResult>('account.resetPassword', {
        email: resetParsed.email,
        token: resetParsed.token,
      });
      setNewPwd(r.newPassword);
      setOkMsg(t('settings.resetDone'));
      onRefresh();
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const links = (
    <div className="card__foot" style={{ gap: 'var(--space-3)' }}>
      {mode !== 'login' && (
        <button type="button" className="link-btn" onClick={() => onSwitchMode('login')}>
          {t('settings.backToLogin')}
        </button>
      )}
      {mode !== 'register' && (
        <button type="button" className="link-btn" onClick={() => onSwitchMode('register')}>
          {t('settings.noAccount')}{t('settings.modeRegister')}
        </button>
      )}
      {mode !== 'reset' && (
        <button type="button" className="link-btn" onClick={() => onSwitchMode('reset')}>
          {t('settings.forgotLink')}
        </button>
      )}
    </div>
  );

  return (
    <>
      {mode === 'login' && (
        <>
          <div className="field">
            <label className="field__label">{t('settings.usernameOrEmail')}</label>
            <input className="input" value={identifier} onChange={(e) => setIdentifier(e.target.value)} />
          </div>
          <div className="field">
            <label className="field__label">{t('settings.password')}</label>
            <input className="input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
          </div>
          <button type="button" className="btn btn-primary" disabled={busy || !identifier || !password} onClick={login}>
            {t('settings.modeLogin')}
          </button>
        </>
      )}

      {mode === 'register' && (
        <>
          <div className="field">
            <label className="field__label">
              {t('settings.email')}
              <span className="muted small"> · {t('settings.emailHint')}</span>
            </label>
            <input
              className={email && !emailOk ? 'input input--bad' : 'input input--big'}
              value={email}
              onChange={(e) => setEmail(e.target.value)}
            />
            {email && !emailOk && <div className="field__err">{t('settings.emailInvalid')}</div>}
          </div>
          <div className="field">
            <label className="field__label">{t('settings.codeLabel')}</label>
            <div className="input-row">
              <input className="input" style={{ flex: 1 }} value={code} onChange={(e) => setCode(e.target.value)} />
              {!emailOk && (
                <span className="muted small" style={{ whiteSpace: 'nowrap' }}>{t('settings.emailFirst')}</span>
              )}
              <button
                type="button"
                className="btn"
                title={emailOk ? t('settings.sendCode') : t('settings.emailFirst')}
                disabled={sending || countdown > 0 || !emailOk}
                onClick={sendCode}
              >
                {sending ? t('settings.sending') : countdown > 0 ? `${countdown}s` : t('settings.sendCode')}
              </button>
            </div>
          </div>
          <div className="field">
            <label className="field__label">
              {t('settings.password')}
              <span className="muted small"> · {t('settings.passwordHint')}</span>
            </label>
            <input className="input" type="password" value={password} onChange={(e) => setPassword(e.target.value)} />
          </div>
          <button
            type="button"
            className="btn btn-primary"
            disabled={busy || !emailOk || !code || password.length < 8}
            onClick={register}
          >
            {t('settings.register')}
          </button>
        </>
      )}

      {mode === 'reset' && (
        <>
          <div className="field">
            <label className="field__label">{t('settings.email')}</label>
            <div className="input-row">
              <input
                className={email && !emailOk ? 'input input--bad' : 'input'}
                style={{ flex: 1 }}
                value={email}
                onChange={(e) => setEmail(e.target.value)}
              />
              <button type="button" className="btn" disabled={!emailOk} onClick={sendResetMail}>
                {t('settings.sendResetMail')}
              </button>
            </div>
          </div>
          <div className="field">
            <label className="field__label">
              {t('settings.resetLinkLabel')}
              <span className="muted small"> · {t('settings.resetLinkHint')}</span>
            </label>
            <input className="input" value={resetLink} onChange={(e) => setResetLink(e.target.value)} />
            {resetLink && !resetParsed && <div className="field__err">{t('settings.resetLinkInvalid')}</div>}
          </div>
          <button type="button" className="btn btn-primary" disabled={busy || !resetParsed} onClick={reset}>
            {t('settings.resetBtn')}
          </button>
        </>
      )}

      {msg && <div className="error-text" style={{ marginTop: 'var(--space-3)' }}>{msg}</div>}
      {okMsg && <div className="hint" style={{ color: 'var(--success)', margin: 'var(--space-3) 0 0' }}>{okMsg}</div>}
      {newPwd && (
        <div className="path-chip" style={{ marginTop: 'var(--space-2)' }}>
          {t('settings.newPassword')}：{newPwd}
        </div>
      )}
      {links}
    </>
  );
}
