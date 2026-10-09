import { useEffect, useState } from 'react';
import { ExternalLink, Info, Ticket, Wallet } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { QuotaSummary, ShopStatus, WalletSummary } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import { fmtUsd } from '@/utils/fmt';

export default function WalletPage({ onNavigate }: PageProps) {
  const [shop, setShop] = useState<ShopStatus | null>(null);
  const [wallet, setWallet] = useState<WalletSummary | null>(null);
  const [walletMsg, setWalletMsg] = useState<string | null>(null);
  const [quota, setQuota] = useState<QuotaSummary | null>(null);

  const load = () => {
    invokeCommand<ShopStatus>('shop.status', {}).then(setShop).catch(() => {});
    invokeCommand<WalletSummary>('shop.wallet', {})
      .then(setWallet)
      .catch((e) => setWalletMsg(errorMessage(e)));
    invokeCommand<QuotaSummary>('usage.getQuota', {}).then(setQuota).catch(() => {});
  };

  useEffect(() => {
    load();
  }, []);

  const openShop = async () => {
    const url = shop?.baseUrl || 'https://shop.greenpool.cn';
    await invokeCommand('app.openExternal', { url }).catch(() => {});
  };

  return (
    <div>
      <div className="page-header__title">{t('wallet.title')}</div>
      <div className="section-subtitle">{t('wallet.subtitle')}</div>

      <div className="banner">
        <Info size={14} />
        {t('wallet.ruleCode')}
      </div>

      <div className="card">
        <div className="card__head">
          <span className="card__label">
            <Wallet size={16} color="var(--muted)" />
            {t('wallet.gpWallet')}
          </span>
          <span className={`status-tag ${shop?.ready ? 'status-tag--ok' : 'status-tag--muted'}`}>
            {shop?.ready ? t('shop.online') : t('shop.notReady')}
          </span>
        </div>
        {wallet ? (
          <>
            <div className="stat-card__value">${wallet.balance}</div>
            {wallet.balanceYuan && (
              <div className="stat-card__sub">≈ ¥{wallet.balanceYuan} {t('wallet.balanceYuanNote')}</div>
            )}
          </>
        ) : walletMsg ? (
          <>
            <div className="stat-card__value">$ --</div>
            <div className="card__desc">{walletMsg}</div>
          </>
        ) : (
          <div className="stat-card__value">$ --</div>
        )}
        <div className="card__desc">{t('wallet.gpDesc')}</div>
        {shop?.ready && <div className="hint" style={{ margin: 'var(--space-2) 0 0' }}>{t('wallet.payInCny')}</div>}
        <div className="card__foot" style={{ gap: 'var(--space-3)' }}>
          <button type="button" className="link-btn" onClick={() => onNavigate('redeem')}>
            <Ticket size={13} style={{ display: 'inline', marginRight: 4, verticalAlign: -2 }} />
            {t('wallet.goRedeem')}
          </button>
          <button type="button" className="link-btn" onClick={openShop}>
            <ExternalLink size={13} style={{ display: 'inline', marginRight: 4, verticalAlign: -2 }} />
            {t('wallet.openShop')}
          </button>
        </div>
      </div>

      <div className="card">
        <div className="card__head">
          <span className="card__label">{t('wallet.serviceBalance')}</span>
        </div>
        <div className="stat-card__value">{quota ? fmtUsd(quota.remainingUsd) : '—'}</div>
        {quota && quota.remainingUsd < 0 && (
          <div className="error-text" style={{ marginTop: 'var(--space-2)' }}>{t('wallet.overdraft')}</div>
        )}
        <div className="card__desc">{t('wallet.serviceDesc')}</div>
        <div className="card__foot">
          <button type="button" className="link-btn" onClick={() => onNavigate('config')}>
            {t('wallet.goUsage')}
          </button>
        </div>
      </div>

      <div className="card">
        <div className="section-title" style={{ margin: 0, fontSize: 14 }}>{t('wallet.subtitle')}</div>
        <div className="row small muted">· {t('wallet.ruleWallet')}</div>
        <div className="row small muted">· {t('wallet.ruleService')}</div>
        <div className="row small muted">· {t('wallet.ruleCode')}</div>
      </div>
    </div>
  );
}
