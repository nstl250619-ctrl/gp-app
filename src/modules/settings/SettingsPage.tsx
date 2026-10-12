// 设置页（§八 设计稿同款）：账号信息卡 + 云端服务/自动更新/版本/退出登录。
// 版本行内置「检查最新版本」按钮：有新版本变「立即更新 vX.X.X」，点击在线更新。
import { useCallback, useEffect, useRef, useState } from 'react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { AppStatus, InstanceCapability } from '@/types/ipc';
import { t } from '@/i18n';
import { checkForUpdate, downloadAndInstall, isAutoUpdateOn, setAutoUpdate } from '@/utils/updater';

type UpdUi = 'idle' | 'checking' | 'uptodate' | 'available' | 'downloading';

export default function SettingsPage() {
  const [app, setApp] = useState<AppStatus | null>(null);
  const [inst, setInst] = useState<InstanceCapability | null>(null);
  const [version, setVersion] = useState('');
  const [auto, setAuto] = useState(false);
  const [updState, setUpdState] = useState<UpdUi>('idle');
  const [updVer, setUpdVer] = useState('');
  const [updErr, setUpdErr] = useState<string | null>(null);
  const resetTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const refresh = useCallback(() => {
    invokeCommand<AppStatus>('app.status', {}).then(setApp).catch(() => {});
    invokeCommand<InstanceCapability>('instance.probe', {}).then(setInst).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
    setAuto(isAutoUpdateOn());
    import('@tauri-apps/api/app').then(({ getVersion }) => getVersion().then(setVersion).catch(() => {}));
  }, [refresh]);

  // 组件卸载时清理「已是最新」的恢复定时器
  useEffect(() => () => {
    if (resetTimer.current) clearTimeout(resetTimer.current);
  }, []);

  const toggleAuto = () => {
    const next = !auto;
    setAuto(next);
    setAutoUpdate(next);
  };

  // 手动检查更新：有新版本 → 按钮变「立即更新 vX.X.X」；无 → 短暂显示「已是最新版本」
  const checkUpdate = async () => {
    if (updState === 'checking' || updState === 'downloading') return;
    setUpdErr(null);
    setUpdState('checking');
    try {
      const u = await checkForUpdate();
      if (u) {
        setUpdVer(u.version);
        setUpdState('available');
      } else {
        setUpdState('uptodate');
        if (resetTimer.current) clearTimeout(resetTimer.current);
        resetTimer.current = setTimeout(() => setUpdState('idle'), 2500);
      }
    } catch (e) {
      setUpdErr(errorMessage(e));
      setUpdState('idle');
    }
  };

  // 在线更新：下载安装并自动重启
  const doUpdate = async () => {
    setUpdErr(null);
    setUpdState('downloading');
    try {
      await downloadAndInstall();
    } catch (e) {
      setUpdErr(errorMessage(e));
      setUpdState('available');
    }
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
          {updState === 'available' || updState === 'downloading' ? (
            <button
              type="button"
              className="upd-btn upd-btn--primary"
              onClick={doUpdate}
              disabled={updState === 'downloading'}
            >
              {updState === 'downloading' ? t('settings.downloading') : `${t('settings.updateNow')} v${updVer}`}
            </button>
          ) : (
            <button
              type="button"
              className="upd-btn"
              onClick={checkUpdate}
              disabled={updState === 'checking'}
            >
              {updState === 'checking'
                ? t('settings.checking')
                : updState === 'uptodate'
                  ? t('settings.latest')
                  : t('settings.checkLatest')}
            </button>
          )}
          <span className="record-row__meta">v{version || '0.1.0'}</span>
        </div>
        {updErr && (
          <div className="record-row" style={{ marginTop: -6 }}>
            <span />
            <span style={{ color: 'var(--danger)', fontSize: 12 }}>{updErr}</span>
          </div>
        )}
        <div className="record-row">
          <span className="muted">{t('settings.logout')}</span>
          <button type="button" className="logout-link" onClick={logout}>{t('settings.logoutAction')}</button>
        </div>
      </div>
    </div>
  );
}
