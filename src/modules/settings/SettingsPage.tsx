import { useCallback, useEffect, useState } from 'react';
import { invokeCommand } from '@/services/ipc';
import type { CredentialStatus, InstanceCapability } from '@/types/ipc';
import { t } from '@/i18n';
import UpdateSection from './UpdateSection';

export default function SettingsPage() {
  const [cred, setCred] = useState<CredentialStatus | null>(null);
  const [inst, setInst] = useState<InstanceCapability | null>(null);

  const refresh = useCallback(() => {
    invokeCommand<CredentialStatus>('credential.status', {}).then(setCred).catch(() => {});
    invokeCommand<InstanceCapability>('instance.probe', {}).then(setInst).catch(() => {});
  }, []);

  useEffect(() => {
    refresh();
  }, [refresh]);

  return (
    <div>
      <div className="page-header__title">{t('settings.title')}</div>

      <div className="section-title">{t('settings.keychain')}</div>
      <div className="card">
        <div className="record-row">
          <span className="muted">{t('settings.keychain')}</span>
          <span className="record-row__meta">
            <span className={`dot ${cred?.available ? 'dot--ok' : 'dot--warn'}`} />
            <span>{cred?.available ? t('settings.keychainAvailable') : t('settings.keychainUnavailable')}</span>
          </span>
        </div>
        <div className="record-row">
          <span className="muted">{t('settings.hasKey')}</span>
          <span>{cred?.hasApiKey ? t('settings.keychainStored') : t('settings.keychainEmpty')}</span>
        </div>
        <div className="hint" style={{ margin: 'var(--space-2) 0 0' }}>{t('settings.keychainNote')}</div>
      </div>

      <div className="section-title">{t('settings.serverAddress')}</div>
      <div className="card">
        <div className="record-row">
          <span>{inst?.serverAddress || inst?.baseUrl || t('common.unknown')}</span>
          {inst && <span className="muted small">v{inst.version}</span>}
        </div>
      </div>

      <UpdateSection />
    </div>
  );
}
