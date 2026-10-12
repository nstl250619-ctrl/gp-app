// Shop recharge modal (GP design system): USD-first preset grid + dual USD/CNY inputs (1:rate),
// QR code rendered locally from code_url (iframe pay page as fallback); polls order status.
import { useEffect, useRef, useState } from 'react';
import QRCode from 'qrcode';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { TopupOrder, TopupStatus, WalletSummary } from '@/types/ipc';
import { t } from '@/i18n';

// USD-first presets: $1/$5/$10/$20/$50/$100 (CNY = usd * rate)
const PRESETS_USD = [1, 5, 10, 20, 50, 100];

interface Props {
  wallet: WalletSummary | null;
  onClose: () => void;
  onPaid: () => void;
}

// round up to 2 decimals
const ceil2 = (n: number) => Math.ceil(n * 100) / 100;

export default function RechargeModal({ wallet, onClose, onPaid }: Props) {
  const rate = Number(wallet?.rate || '7') || 7;
  const [usd, setUsd] = useState('10.00'); // left input ($)
  const [cny, setCny] = useState('70.00'); // right input (CNY)
  const [channel, setChannel] = useState<'wxpay' | 'alipay'>('wxpay');
  const [busy, setBusy] = useState(false);
  const [order, setOrder] = useState<TopupOrder | null>(null);
  const [qrData, setQrData] = useState('');
  const [msg, setMsg] = useState<string | null>(null);
  const [paid, setPaid] = useState(false);
  const timer = useRef<ReturnType<typeof setInterval> | null>(null);
  const closeTimer = useRef<ReturnType<typeof setTimeout> | null>(null);

  const cnyNum = Number(cny);
  // backend validates by CNY amount (1 ~ 100000)
  const valid = Number.isFinite(cnyNum) && cnyNum >= 1 && cnyNum <= 100000;

  useEffect(() => () => {
    if (timer.current) clearInterval(timer.current);
    if (closeTimer.current) clearTimeout(closeTimer.current);
  }, []);

  const stopPoll = () => { if (timer.current) { clearInterval(timer.current); timer.current = null; } };

  // dual inputs: either side edits, the other auto-converts (1:rate, ceil to 2 decimals)
  const onUsd = (v: string) => {
    setUsd(v);
    const n = Number(v);
    if (v !== '' && Number.isFinite(n) && n > 0) setCny(ceil2(n * rate).toFixed(2));
    if (v === '') setCny('');
  };
  const onCny = (v: string) => {
    setCny(v);
    const n = Number(v);
    if (v !== '' && Number.isFinite(n) && n > 0) setUsd(ceil2(n / rate).toFixed(2));
    if (v === '') setUsd('');
  };
  const pickPreset = (p: number) => {
    setUsd(p.toFixed(2));
    setCny(ceil2(p * rate).toFixed(2));
  };

  const pay = async () => {
    if (!valid || busy) return;
    setBusy(true);
    setMsg(null);
    try {
      const r = await invokeCommand<TopupOrder>('shop.topupCreate', { amountCny: cnyNum.toFixed(2), channel });
      setOrder(r);
      if (r.codeUrl) {
        setQrData(await QRCode.toDataURL(r.codeUrl, { width: 240, margin: 1 }));
      }
      // poll payment status (5s), until order expires (server-provided TTL)
      timer.current = setInterval(async () => {
        try {
          const s = await invokeCommand<TopupStatus>('shop.topupStatus', { orderId: r.orderId });
          if (s.status === 'paid') {
            stopPoll();
            setPaid(true);
            onPaid();
            closeTimer.current = setTimeout(onClose, 5000);
          } else if (s.status === 'expired' || Date.now() / 1000 > r.expiresAt) {
            stopPoll();
            setOrder(null);
            setQrData('');
            setMsg(t('mall.rechargeExpired'));
          }
        } catch { /* poll failure ignored, retried next tick */ }
      }, 5000);
    } catch (e) {
      setMsg(errorMessage(e));
    } finally {
      setBusy(false);
    }
  };

  const sub = t('mall.rechargeSub').replace('{rate}', String(rate));

  return (
    <div className="modal-mask" onClick={order ? undefined : onClose}>
      <div className="recharge" onClick={(e) => e.stopPropagation()}>
        {paid ? (
          <div className="recharge__paid">
            <div>{t('mall.rechargePaid')}</div>
            <button type="button" className="btn btn-primary" style={{ marginTop: 14 }} onClick={onClose}>
              {t('mall.rechargeClose')}
            </button>
          </div>
        ) : order ? (
          <>
            <div className="recharge__title">{t('mall.rechargeTitle')}</div>
            <div className="recharge__sub">{t('mall.rechargeScan')}</div>
            {qrData ? (
              <div className="recharge__qrbox">
                <img className="recharge__qrimg" src={qrData} alt="qrcode" />
                <div className="recharge__paymeta">
                  {t('mall.rechargePay')} ¥{cny} · {t('mall.rechargeApprox')} ${usd}
                </div>
              </div>
            ) : (
              <iframe className="recharge__qr" src={order.payUrl} title="pay" />
            )}
            <div className="recharge__open">
              <button type="button" className="recharge__link" onClick={() => invokeCommand('app.openExternal', { url: order.payUrl }).catch(() => {})}>
                {t('mall.rechargeOpenBrowser')}
              </button>
              <button type="button" className="recharge__link" onClick={() => { stopPoll(); setOrder(null); setQrData(''); }}>
                {t('mall.rechargeBack')}
              </button>
            </div>
          </>
        ) : (
          <>
            <div className="recharge__title">{t('mall.rechargeTitle')}</div>
            <div className="recharge__sub">{sub}</div>

            <div className="recharge__grid">
              {PRESETS_USD.map((p) => (
                <button
                  type="button"
                  key={p}
                  className={`recharge__preset ${usd === p.toFixed(2) ? 'recharge__preset--on' : ''}`}
                  onClick={() => pickPreset(p)}
                >
                  <span className="recharge__preset-amt">${p.toFixed(2)}</span>
                  <span className="recharge__preset-get">{t('mall.rechargePay')} ¥{(p * rate).toFixed(2)}</span>
                </button>
              ))}
            </div>

            <div className="recharge__label">{t('mall.rechargeCustom')}</div>
            <div className="recharge__duo">
              <div className="recharge__field">
                <span className="recharge__unit">$</span>
                <input
                  value={usd}
                  inputMode="decimal"
                  onChange={(e) => onUsd(e.target.value.replace(/[^\d.]/g, ''))}
                />
              </div>
              <span className="recharge__swap">⇄</span>
              <div className="recharge__field">
                <span className="recharge__unit">¥</span>
                <input
                  value={cny}
                  inputMode="decimal"
                  onChange={(e) => onCny(e.target.value.replace(/[^\d.]/g, ''))}
                />
              </div>
            </div>

            <div className="recharge__label">{t('mall.rechargeMethod')}</div>
            <div className="recharge__channels">
              <button
                type="button"
                className={`recharge__ch ${channel === 'wxpay' ? 'recharge__ch--on' : ''}`}
                onClick={() => setChannel('wxpay')}
              >
                {t('mall.rechargeWx')}
              </button>
              <button
                type="button"
                className={`recharge__ch ${channel === 'alipay' ? 'recharge__ch--on' : ''}`}
                onClick={() => setChannel('alipay')}
              >
                {t('mall.rechargeAli')}
              </button>
            </div>

            {msg && <div className="recharge__err">{msg}</div>}

            <div className="recharge__row">
              <button type="button" className="btn recharge__cancel" onClick={onClose}>{t('mall.rechargeCancel')}</button>
              <button type="button" className="btn btn-primary recharge__go" disabled={busy || !valid} onClick={pay}>
                {busy ? t('mall.buying') : `${t('mall.rechargeGo')} ¥${valid ? cnyNum.toFixed(2) : '--'}`}
              </button>
            </div>
          </>
        )}
      </div>
    </div>
  );
}
