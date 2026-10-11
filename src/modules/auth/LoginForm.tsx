// 登录卡片表单（登录/注册/找回三态），复用 account 命令。状态自含，成功后回调 onSuccess。
import { useEffect, useState } from 'react';
import { errorMessage, invokeCommand, isIpcError } from '@/services/ipc';
import type { AccountStatus } from '@/types/ipc';
import { t } from '@/i18n';
import { isValidEmail } from '@/utils/time';

export type AuthMode = 'login' | 'register' | 'reset';

interface Props {
  mode: AuthMode;
  onSwitchMode: (m: AuthMode) => void;
  onSuccess: (username: string) => void;
}

export default function LoginForm({ mode, onSwitchMode, onSuccess }: Props) {
  const [identifier, setIdentifier] = useState('');
  const [email, setEmail] = useState('');
  const [code, setCode] = useState('');
  const [password, setPassword] = useState('');
  const [newPassword, setNewPassword] = useState('');
  const [countdown, setCountdown] = useState(0);
  const [msg, setMsg] = useState<string | null>(null);
  const [okMsg, setOkMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [sending, setSending] = useState(false);

  const emailOk = isValidEmail(email);

  useEffect(() => {
    if (countdown <= 0) return;
    const timer = window.setInterval(() => setCountdown((c) => c - 1), 1000);
    return () => window.clearInterval(timer);
  }, [countdown]);

  const login = async () => {
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      const s = await invokeCommand<AccountStatus>('account.login', { username: identifier, password });
      setPassword('');
      onSuccess(s.username);
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
      setCountdown(30);
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
      const s = await invokeCommand<AccountStatus>('account.register', { email, password, code });
      setPassword('');
      setCode('');
      onSuccess(s.username);
    } catch (e) {
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
    if (sending) return;
    setSending(true);
    setMsg(null);
    setOkMsg(null);
    try {
      await invokeCommand('account.sendPasswordReset', { email });
      setOkMsg(t('settings.codeSent'));
      setCountdown(30);
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setSending(false);
    }
  };

  const reset = async () => {
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      const s = await invokeCommand<AccountStatus>('account.resetPassword', {
        email,
        code,
        newPassword,
      });
      setCode('');
      setNewPassword('');
      onSuccess(s.username);
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const switchMode = (m: AuthMode) => {
    setMsg(null);
    setOkMsg(null);
    onSwitchMode(m);
  };

  return (
    <>
      {mode === 'login' && (
        <>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.usernameOrEmail')}</label>
            <input className="login-card__input" value={identifier} onChange={(e) => setIdentifier(e.target.value)} />
          </div>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.password')}</label>
            <input
              className="login-card__input"
              type="password"
              value={password}
              placeholder={t('login.passwordPlaceholder')}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => { if (e.key === 'Enter' && !busy && identifier && password) login(); }}
            />
          </div>
          <button type="button" className="login-card__btn" disabled={busy || !identifier || !password} onClick={login}>
            {busy ? t('login.loggingIn') : t('login.btnLogin')}
          </button>
        </>
      )}

      {mode === 'register' && (
        <>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.email')}</label>
            <input
              className="login-card__input"
              value={email}
              placeholder={t('login.emailPlaceholder')}
              onChange={(e) => setEmail(e.target.value)}
            />
            {email && !emailOk && <div className="login-card__error">{t('settings.emailInvalid')}</div>}
          </div>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.codeLabel')}</label>
            <div className="login-card__code">
              <input
                className="login-card__input"
                value={code}
                placeholder={t('login.codePlaceholder')}
                onChange={(e) => setCode(e.target.value)}
              />
              <button
                type="button"
                className="login-card__send"
                disabled={sending || countdown > 0 || !emailOk}
                onClick={sendCode}
              >
                {sending ? t('settings.sending') : countdown > 0 ? `${countdown}s` : t('settings.sendCode')}
              </button>
            </div>
          </div>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.password')}</label>
            <input
              className="login-card__input"
              type="password"
              placeholder={t('settings.passwordHint')}
              value={password}
              onChange={(e) => setPassword(e.target.value)}
            />
          </div>
          <button
            type="button"
            className="login-card__btn"
            disabled={busy || !emailOk || !code || password.length < 8}
            onClick={register}
          >
            {busy ? t('login.loggingIn') : t('login.btnRegister')}
          </button>
        </>
      )}

      {mode === 'reset' && (
        <>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.email')}</label>
            <div className="login-card__code">
              <input
                className="login-card__input"
                value={email}
                placeholder={t('login.emailPlaceholder')}
                onChange={(e) => setEmail(e.target.value)}
              />
              <button
                type="button"
                className="login-card__send"
                disabled={sending || countdown > 0 || !emailOk}
                onClick={sendResetMail}
              >
                {sending ? t('settings.sending') : countdown > 0 ? `${countdown}s` : t('settings.sendCode')}
              </button>
            </div>
            {email && !emailOk && <div className="login-card__error">{t('settings.emailInvalid')}</div>}
          </div>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.codeLabel')}</label>
            <input
              className="login-card__input"
              value={code}
              placeholder={t('login.codePlaceholder')}
              onChange={(e) => setCode(e.target.value)}
            />
          </div>
          <div className="login-card__field">
            <label className="login-card__label">{t('settings.newPassword')}</label>
            <input
              className="login-card__input"
              type="password"
              placeholder={t('settings.passwordHint')}
              value={newPassword}
              onChange={(e) => setNewPassword(e.target.value)}
            />
          </div>
          <button
            type="button"
            className="login-card__btn"
            disabled={busy || !emailOk || !code || newPassword.length < 8}
            onClick={reset}
          >
            {busy ? t('login.loggingIn') : t('settings.resetBtn')}
          </button>
        </>
      )}

      {msg && <div className="login-card__error">{msg}</div>}
      {okMsg && <div className="login-card__hint">{okMsg}</div>}

      <div className="login-card__foot">
        {mode === 'login' && (
          <>
            <button type="button" className="login-card__link" onClick={() => switchMode('register')}>
              {t('settings.noAccount')}<b>{t('login.goRegister')}</b>
            </button>
            <button type="button" className="login-card__link" onClick={() => switchMode('reset')}>
              {t('settings.forgotLink')}
            </button>
          </>
        )}
        {mode === 'register' && (
          <button type="button" className="login-card__link" onClick={() => switchMode('login')}>
            {t('settings.hasAccount')}<b>{t('login.goLogin')}</b>
          </button>
        )}
        {mode === 'reset' && (
          <button type="button" className="login-card__link" onClick={() => switchMode('login')}>
            {t('settings.backToLogin')}
          </button>
        )}
      </div>
    </>
  );
}
