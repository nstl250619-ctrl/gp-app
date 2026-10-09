import { useCallback, useEffect, useState } from 'react';
import { RefreshCw } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { InstallRecord } from '@/types/ipc';
import { t } from '@/i18n';
import { formatRelative } from '@/utils/time';

function statusText(status: string): string {
  if (status === 'rolled_back') return t('records.statusRolledBack');
  if (status === 'connectivity_failed') return t('records.statusConnectivity');
  if (status === 'failed') return t('records.statusFailed');
  return t('records.statusApplied');
}

export default function RecordsPage() {
  const [records, setRecords] = useState<InstallRecord[] | null>(null);
  const [msg, setMsg] = useState<string | null>(null);

  const load = useCallback(() => {
    invokeCommand<InstallRecord[]>('install.history', {}).then(setRecords).catch((e) => setMsg(errorMessage(e)));
  }, []);

  useEffect(() => {
    load();
  }, [load]);

  const rollback = async (id: string) => {
    setMsg(null);
    try {
      await invokeCommand('install.rollback', { recordId: id });
      load();
    } catch (e) {
      setMsg(errorMessage(e));
    }
  };

  return (
    <div>
      <div className="page-header">
        <div className="page-header__title">{t('records.title')}</div>
        <button type="button" className="link-btn" style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }} onClick={load}>
          <RefreshCw size={14} />
          {t('common.refresh')}
        </button>
      </div>
      {msg && <div className="error-text">{msg}</div>}
      <div className="card">
        {records && records.length === 0 && <p className="muted">{t('records.empty')}</p>}
        {records?.map((h) => (
          <div className="record-row" key={h.id}>
            <span>
              <span style={{ fontWeight: 600 }}>{h.target}</span>
              <span className="muted" style={{ marginLeft: 'var(--space-2)' }}>{statusText(h.status)}</span>
            </span>
            <span className="record-row__meta">
              <span>{formatRelative(new Date(h.createdAt))}</span>
              {h.backupPath && (
                <button
                  type="button"
                  className="link-btn"
                  disabled={h.status === 'rolled_back'}
                  style={{ marginLeft: 'var(--space-2)' }}
                  onClick={() => rollback(h.id)}
                >
                  {t('records.rollback')}
                </button>
              )}
            </span>
          </div>
        ))}
        {!records && <p className="muted">{t('common.loading')}</p>}
      </div>
    </div>
  );
}
