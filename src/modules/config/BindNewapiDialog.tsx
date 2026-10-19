// 中转站账号绑定引导（《联合整改需求单 v2》3.3 / 4.2）：
// 商城返回 need_bind_newapi / need_rebind_newapi 时弹出，用户输一次邮箱+密码即可自动登录。
import { useEffect, useState } from 'react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { AccountStatus } from '@/types/ipc';
import { t } from '@/i18n';

interface Props {
  open: boolean;
  onClose: () => void;
  onBound: () => void;
}

export default function BindNewapiDialog({ open, onClose, onBound }: Props) {
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [busy, setBusy] = useState(false);
  const [err, setErr] = useState<string | null>(null);

  // 打开时预填当前账号邮箱（可在输入框里修改）
  useEffect(() => {
    if (!open) return;
    if (username.trim()) return;
    invokeCommand<AccountStatus>('account.status', {})
      .then((s) => {
        if (s.username) setUsername(s.username);
      })
      .catch(() => {});
  }, [open, username]);

  if (!open) return null;

  const submit = async () => {
    if (!username.trim() || !password) {
      setErr(t('bind.invalid'));
      return;
    }
    setBusy(true);
    setErr(null);
    try {
      await invokeCommand('account.bindNewapi', { username: username.trim(), password });
      setPassword('');
      onBound();
    } catch (e) {
      setErr(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div className="modal-mask" onClick={onClose}>
      <div className="recharge" onClick={(e) => e.stopPropagation()}>
        <div className="recharge__title">{t('bind.title')}</div>
        <div className="recharge__sub">{t('bind.sub')}</div>
        <div style={{ marginTop: 'var(--space-4)', display: 'grid', gap: 8 }}>
          <input
            className="input"
            placeholder={t('bind.username')}
            value={username}
            onChange={(e) => setUsername(e.target.value)}
            autoFocus
          />
          <input
            className="input"
            type="password"
            placeholder={t('bind.password')}
            value={password}
            onChange={(e) => setPassword(e.target.value)}
            onKeyDown={(e) => {
              if (e.key === 'Enter') void submit();
            }}
          />
        </div>
        {err && (
          <p className="muted" style={{ color: 'var(--danger)', marginTop: 8 }}>
            {err}
          </p>
        )}
        <div style={{ display: 'flex', gap: 8, marginTop: 'var(--space-4)', justifyContent: 'flex-end' }}>
          <button type="button" className="btn btn--ghost" onClick={onClose} disabled={busy}>
            {t('bind.later')}
          </button>
          <button type="button" className="btn btn-primary" onClick={submit} disabled={busy}>
            {busy ? t('bind.submitting') : t('bind.submit')}
          </button>
        </div>
      </div>
    </div>
  );
}
