// 设置页·软件更新模块：自动检测开关 + 手动检查/更新。
import { useEffect, useState } from 'react';
import { Download, RefreshCw } from 'lucide-react';
import { t } from '@/i18n';
import {
  checkForUpdate, downloadAndInstall, isAutoUpdateOn, setAutoUpdate, type UpdateInfo,
} from '@/utils/updater';

type State = 'idle' | 'checking' | 'latest' | 'available' | 'downloading' | 'relaunching' | 'error';

export default function UpdateSection() {
  const [auto, setAuto] = useState(false);
  const [current, setCurrent] = useState('');
  const [state, setState] = useState<State>('idle');
  const [info, setInfo] = useState<UpdateInfo | null>(null);

  useEffect(() => {
    setAuto(isAutoUpdateOn());
    import('@tauri-apps/api/app').then(({ getVersion }) =>
      getVersion().then(setCurrent).catch(() => {}),
    );
  }, []);

  const toggle = () => {
    const next = !auto;
    setAuto(next);
    setAutoUpdate(next);
  };

  const check = async () => {
    setState('checking');
    setInfo(null);
    try {
      const u = await checkForUpdate();
      if (u) {
        setInfo(u);
        setState('available');
      } else {
        setState('latest');
      }
    } catch {
      setState('error');
    }
  };

  const install = async () => {
    setState('downloading');
    try {
      await downloadAndInstall();
      setState('relaunching');
    } catch {
      setState('error');
    }
  };

  return (
    <>
      <div className="section-title">{t('settings.updateTitle')}</div>
      <div className="card">
        <div className="record-row">
          <span className="muted">{t('settings.currentVersion')}</span>
          <span>v{current || '0.1.0'}</span>
        </div>

        <div className="record-row">
          <span className="muted">{t('settings.autoUpdate')}</span>
          <button type="button" className="toggle" role="switch" aria-checked={auto} onClick={toggle}>
            <span className={`toggle__track ${auto ? 'toggle__track--on' : ''}`}>
              <span className="toggle__thumb" />
            </span>
            <span className="small">{auto ? t('common.on') : t('common.off')}</span>
          </button>
        </div>
        <div className="hint" style={{ margin: 'var(--space-2) 0 0' }}>
          {auto ? t('settings.autoUpdateOn') : t('settings.autoUpdateOff')}
        </div>

        {info && state === 'available' && (
          <div className="banner" style={{ marginTop: 'var(--space-3)' }}>
            {t('settings.available')} v{info.version}
          </div>
        )}

        <div style={{ display: 'flex', alignItems: 'center', gap: 'var(--space-3)', marginTop: 'var(--space-3)' }}>
          <button
            type="button"
            className="btn btn-primary"
            disabled={state === 'checking' || state === 'downloading'}
            onClick={state === 'available' ? install : check}
          >
            {state === 'downloading' || state === 'relaunching'
              ? <RefreshCw size={15} />
              : <Download size={15} />}
            {state === 'checking' && t('settings.checking')}
            {state === 'downloading' && t('settings.downloading')}
            {state === 'relaunching' && t('settings.relaunching')}
            {state === 'latest' && t('settings.latest')}
            {state === 'error' && t('settings.updateError')}
            {(state === 'idle' || state === 'available') && (state === 'available' ? t('settings.updateNow') : t('settings.checkUpdate'))}
          </button>
          {state === 'latest' && <span className="muted small">{t('settings.latest')}</span>}
        </div>
      </div>
    </>
  );
}
