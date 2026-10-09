import { useEffect, useState } from 'react';
import { ClipboardPaste } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { RedemptionRecord, RedeemResult } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import { formatUnix } from '@/utils/time';
import { fmtUsd } from '@/utils/fmt';

export default function RedeemPage({ onNavigate }: PageProps) {
  const [code, setCode] = useState('');
  const [msg, setMsg] = useState<string | null>(null);
  const [okMsg, setOkMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  // 兑换记录：logs type=1（兑换码核销不写 top_ups 表，从日志取）
  const [records, setRecords] = useState<RedemptionRecord[] | null>(null);

  const loadRecords = () => {
    invokeCommand<RedemptionRecord[]>('usage.redemptionRecords', {})
      .then(setRecords)
      .catch(() => setRecords([]));
  };

  useEffect(() => {
    loadRecords();
  }, []);

  const paste = async () => {
    try {
      const text = await navigator.clipboard.readText();
      if (text) setCode(text.trim());
    } catch {
      setMsg(t('common.unknownError'));
    }
  };

  const submit = async () => {
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      const r = await invokeCommand<RedeemResult>('redeem.redeem', { code });
      setOkMsg(`${t('redeem.success')}（+${fmtUsd(r.grantedUsd)}）`);
      setCode('');
      loadRecords();
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <div className="page-header__title">{t('redeem.title')}</div>
      <div className="section-subtitle">{t('redeem.subtitle')}</div>

      <div className="field">
        <label className="field__label">{t('redeem.codeLabel')}</label>
        <div className="input-row">
          <input
            className="input input--big"
            style={{ flex: 1 }}
            value={code}
            placeholder={t('redeem.codePlaceholder')}
            onChange={(e) => setCode(e.target.value)}
          />
          <button type="button" className="icon-btn" onClick={paste} title={t('common.copy')}>
            <ClipboardPaste size={16} />
          </button>
        </div>
      </div>
      <div className="hint">{t('redeem.codeHelp')}</div>

      {msg && <div className="error-text">{msg}</div>}
      {okMsg && <div className="hint" style={{ color: 'var(--success)' }}>{okMsg}</div>}

      <button
        type="button"
        className="btn btn-primary btn-big"
        disabled={busy || code.trim().length === 0}
        onClick={submit}
      >
        {t('redeem.submit')}
      </button>

      <div className="page-header" style={{ marginTop: 'var(--space-6)', marginBottom: 0 }}>
        <div className="section-title" style={{ margin: 0 }}>{t('redeem.recentTitle')}</div>
        <button type="button" className="link-btn" onClick={() => onNavigate('records')}>
          {t('common.viewAll')}
        </button>
      </div>
      <div className="card">
        {records && records.length === 0 && <p className="muted">{t('redeem.empty')}</p>}
        {records?.map((r, i) => (
          <div className="record-row" key={i}>
            <span className="record-row__amount">
              + {r.amountUsd !== null ? fmtUsd(r.amountUsd) : ''}
            </span>
            <span className="record-row__meta">
              <span>{formatUnix(r.createdAt)}</span>
            </span>
          </div>
        ))}
        {!records && <p className="muted">{t('common.loading')}</p>}
      </div>
    </div>
  );
}
