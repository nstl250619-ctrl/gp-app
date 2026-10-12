// 兑换页（§八 设计稿同款）：输入框+按钮同行卡片 + 绿色到账提示条 + 兑换记录。
import { useEffect, useState } from 'react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { RedemptionRecord, RedeemResult } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import { formatDateTime, formatUnix } from '@/utils/time';
import { fmtUsd } from '@/utils/fmt';

export default function RedeemPage(_props: PageProps) {
  const [code, setCode] = useState('');
  const [msg, setMsg] = useState<string | null>(null);
  const [okMsg, setOkMsg] = useState<string | null>(null);
  const [busy, setBusy] = useState(false);
  const [records, setRecords] = useState<RedemptionRecord[] | null>(null);

  const loadRecords = () => {
    invokeCommand<RedemptionRecord[]>('usage.redemptionRecords', {})
      .then(setRecords)
      .catch(() => setRecords([]));
  };

  useEffect(() => {
    loadRecords();
  }, []);

  const submit = async () => {
    setBusy(true);
    setMsg(null);
    setOkMsg(null);
    try {
      const r = await invokeCommand<RedeemResult>('redeem.redeem', { code });
      setOkMsg(
        r.subscriptionActive
          ? `${t('redeem.trialHint')}（+${fmtUsd(r.grantedUsd)}）`
          : `${t('redeem.success')}（+${fmtUsd(r.grantedUsd)}）`,
      );
      setCode('');
      loadRecords();
      // 服务余额已变，通知顶栏刷新
      window.dispatchEvent(new CustomEvent('gp:balance-changed'));
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <div>
      <h1>{t('redeem.title')}</h1>
      <div className="sub">{t('redeem.subtitle')}</div>

      <div className="card">
        <div className="redeem-row">
          <input
            className="input input--big"
            style={{ flex: 1 }}
            value={code}
            placeholder={t('redeem.codePlaceholder')}
            onChange={(e) => setCode(e.target.value)}
            onKeyDown={(e) => { if (e.key === 'Enter' && !busy && code.trim()) submit(); }}
          />
          <button type="button" className="btn btn-primary" disabled={busy || code.trim().length === 0} onClick={submit}>
            {t('redeem.submit')}
          </button>
        </div>
        {msg && <div className="error-text" style={{ marginTop: 'var(--space-3)', marginBottom: 0 }}>{msg}</div>}
        {okMsg && <div className="apply-banner apply-banner--ok" style={{ marginTop: 'var(--space-3)' }}>{okMsg}</div>}
      </div>

      <div className="card">
        <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>{t('redeem.recentTitle')}</div>
        {records && records.length === 0 && <p className="muted">{t('redeem.empty')}</p>}
        {records?.map((r, i) => (
          <div className="record-row" key={i}>
            <span className="record-row__amount">
              + {r.amountUsd !== null ? fmtUsd(r.amountUsd) : ''}
            </span>
            <span className="record-row__meta">
              <span>
                {r.expiresAt === 0
                  ? t('redeem.neverExpires')
                  : r.expiresAt != null
                    ? `${t('redeem.expiresAt')} ${formatDateTime(r.expiresAt)}`
                    : ''}
              </span>
              <span>{formatUnix(r.createdAt)}</span>
            </span>
          </div>
        ))}
        {!records && <p className="muted">{t('common.loading')}</p>}
      </div>
    </div>
  );
}
