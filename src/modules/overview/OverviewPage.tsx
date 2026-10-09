import { useCallback, useEffect, useState } from 'react';
import { Archive, RefreshCw } from 'lucide-react';
import { invokeCommand } from '@/services/ipc';
import type { AppStatus, DetectResult, InstanceCapability } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import { formatRelative } from '@/utils/time';
import AccountModule from './AccountModule';
import KeyModule from './KeyModule';

export default function OverviewPage({ onNavigate }: PageProps) {
  const [app, setApp] = useState<AppStatus | null>(null);
  const [detect, setDetect] = useState<DetectResult | null>(null);
  const [probe, setProbe] = useState<InstanceCapability | null>(null);
  const [refreshedAt, setRefreshedAt] = useState<Date>(new Date());
  const [, forceTick] = useState(0);

  const load = useCallback(() => {
    setRefreshedAt(new Date());
    invokeCommand<AppStatus>('app.status', {}).then(setApp).catch(() => {});
    invokeCommand<DetectResult[]>('install.detect', {})
      .then((list) => setDetect(list[0] ?? null))
      .catch(() => {});
    invokeCommand<InstanceCapability>('instance.probe', {}).then(setProbe).catch(() => {});
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  // 每分钟重算一次「N 分钟前」
  useEffect(() => {
    const timer = window.setInterval(() => forceTick((x) => x + 1), 60000);
    return () => window.clearInterval(timer);
  }, []);

  return (
    <div>
      <div className="page-header">
        <div className="page-header__title">{t('overview.title')}</div>
        <div className="page-header__actions">
          <button type="button" className="link-btn" style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }} onClick={load}>
            <RefreshCw size={14} />
            {t('common.refresh')}
          </button>
          <span>{formatRelative(refreshedAt)}</span>
        </div>
      </div>
      <div className="section-title">{t('overview.currentStatus')}</div>
      {probe && !probe.reachable && <div className="banner">{t('statusbar.probeFail')}</div>}

      <div className="overview-grid">
        <AccountModule account={app} onRefresh={load} />

        <KeyModule account={app} onRefresh={load} />

        <div className="card">
          <div className="card__head">
            <span className="card__label">
              <Archive size={16} color="var(--muted)" />
              {t('overview.localConfig')}
            </span>
            <span className="card__state">
              <span className={`dot ${detect?.hasManagedEntry ? 'dot--ok' : ''}`} />
              {detect?.hasManagedEntry ? t('overview.configDone') : t('overview.notStarted')}
            </span>
          </div>
          <div className="card__big">{detect?.hasManagedEntry ? t('overview.configDone') : t('overview.configNone')}</div>
          <div className="card__desc">{detect?.hasManagedEntry ? detect.path || '' : t('overview.configDesc')}</div>
          {!detect?.hasManagedEntry && (
            <div className="card__foot">
              <button type="button" className="link-btn" onClick={() => onNavigate('config')}>
                {t('overview.actionConfig')}
              </button>
            </div>
          )}
        </div>
      </div>

      {!app && <p className="muted">{t('common.loading')}</p>}
    </div>
  );
}
