// 设置页（§八 设计稿同款）：账号信息卡 + 云端服务/自动更新/版本/退出登录。
import { useCallback, useEffect, useState } from 'react';
import { invokeCommand } from '@/services/ipc';
import type { AppStatus, InstanceCapability } from '@/types/ipc';
import { t } from '@/i18n';
import { isAutoUpdateOn, setAutoUpdate } from '@/utils/updater';

export default function SettingsPage() {
  const [app, setApp] = useState<AppStatus | null>(null);
  const [inst, setInst] = useState<InstanceCapability | null>(null);
  const [version, setVersion] = useState('');
  const [auto, setAuto] = useState(false);

  const refresh = useCallback(() => {
    invokeCommand<AppStatus>('app.status', {}).then(setApp).catch(() => {});
    invokeCommand<InstanceCapability>('instance.probe', {}).then(setInst).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    setAuto(isAutoUpdateOn());
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion().then(setVersion).catch(() => {}));
  }, [refresh]);

  const toggleAuto = () => {
    const next = !auto;
    setAuto(next);
    setAutoUpdate(next);
  };

  const logout = async () => {
    await invokeCommand('account.logout', {}).catch(() => {});
    // 整页重载：重新走 app.status 探测 → 回到登录门面
    window.location.reload();
  };

  return (
    <div>
      <h1>{t('settings.title')}</h1>
      <div className="sub">{t('settings.subtitle')}</div>

      <div className="card">
        <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 6 }}>{t('settings.accountSection')}</div>
        <div className="record-row">
          <span className="muted">{t('account.email')}</span>
          <span>{app?.loggedIn ? app.username : t('account.loggedOut')}</span>
        </div>
        <div className="record-row">
          <span className="muted">{t('account.role')}</span>
          <span>{t('account.roleUser')}</span>
        </div>
        <div className="record-row">
          <span className="muted">{t('account.status')}</span>
          <span className="record-row__meta">
            <span className={`dot ${app?.loggedIn ? 'dot--ok' : 'dot--warn'}`} />
            <span style={app?.loggedIn ? { color: 'var(--success)' } : undefined}>
              {app?.loggedIn ? t('account.loggedIn') : t('account.loggedOut')}
            </span>
          </span>
        </div>
      </div>

      <div className="card">
        <div className="record-row">
          <span className="muted">{t('settings.service')}</span>
          <span className="record-row__meta">
            <span>{inst?.serverAddress || inst?.baseUrl || t('common.unknown')}</span>
            {inst?.version && (
              <span className="ver-chip">v{inst.version}</span>
            )}
          </span>
        </div>
        <div className="record-row">
          <span className="muted">{t('settings.autoUpdate')}</span>
          <button type="button" className="toggle" role="switch" aria-checked={auto} onClick={toggleAuto}>
            <span className={`toggle__track ${auto ? 'toggle__track--on' : ''}`}>
              <span className="toggle__thumb" />
            </span>
          </button>
        </div>
        <div className="record-row">
          <span className="muted">{t('settings.currentVersion')}</span>
          <span>v{version || '0.1.0'}</span>
        </div>
        <div className="record-row">
          <span className="muted">{t('settings.logout')}</span>
          <button type="button" className="logout-link" onClick={logout}>{t('settings.logoutAction')}</button>
        </div>
      </div>
    </div>
  );
}
