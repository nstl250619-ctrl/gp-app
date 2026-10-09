import { useEffect, useState } from 'react';
import { ChevronLeft, ExternalLink, Ticket } from 'lucide-react';
import { invokeCommand } from '@/services/ipc';
import type { ShopStatus } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';

export default function ShopPage({ onNavigate }: PageProps) {
  const [shop, setShop] = useState<ShopStatus | null>(null);
  const [embedSrc, setEmbedSrc] = useState<string>('');

  useEffect(() => {
    invokeCommand<ShopStatus>('shop.status', {})
      .then((s) => {
        setShop(s);
        if (s.ready && s.ssoReady) {
          // 方案 A：SSO 就绪 → 每次开门重铸新 token（兼容一次性模式），失败回落普通地址
          invokeCommand<string>('shop.ssoUrl', {})
            .then(setEmbedSrc)
            .catch(() => setEmbedSrc(s.baseUrl));
        } else {
          setEmbedSrc(s.baseUrl);
        }
      })
      .catch(() => {});
  }, []);

  const openShop = async () => {
    const url = shop?.baseUrl || 'https://shop.greenpool.cn';
    await invokeCommand('app.openExternal', { url }).catch(() => {});
  };

  return (
    <div className="shop-page">
      <div className="shop-toolbar">
        <button type="button" className="link-btn" style={{ display: 'inline-flex', alignItems: 'center', gap: 4 }} onClick={() => onNavigate('overview')}>
          <ChevronLeft size={16} />
          {t('shop.back')}
        </button>
        <span className="shop-toolbar__title">{t('nav.shop')}</span>
        <span className="muted small">· {t('shop.domain')}</span>
        <span
          style={{
            marginLeft: 'var(--space-4)',
            color: 'var(--danger)',
            fontWeight: 600,
            fontSize: 14,
          }}
        >
          {t('shop.buyHint')}
        </span>
        <span className="shop-toolbar__spacer" />
        <button type="button" className="btn" onClick={() => onNavigate('redeem')}>
          <Ticket size={15} />
          {t('shop.quickRedeem')}
        </button>
        <button type="button" className="icon-btn" title={t('shop.openWeb')} onClick={openShop}>
          <ExternalLink size={14} />
        </button>
      </div>

      {shop?.ready ? (
        <>
          {embedSrc ? (
            <iframe src={embedSrc} className="shop-embed" title={t('shop.domain')} />
          ) : (
            <p className="muted">{t('common.loading')}</p>
          )}
          <div className="hint" style={{ marginTop: 'var(--space-2)' }}>
            {t('shop.autoLoginNote')} · {t('shop.embedFallback')}
            <button type="button" className="link-btn" style={{ marginLeft: 'var(--space-2)' }} onClick={openShop}>
              {t('shop.openWeb')}
            </button>
          </div>
        </>
      ) : (
        <div className="shop-frame">
          <div className="card__big">{t('shop.notReady')}</div>
          <div className="card__desc">{shop?.message || t('shop.notReadyDesc')}</div>
          <button type="button" className="btn btn-primary" style={{ marginTop: 'var(--space-3)' }} onClick={openShop}>
            {t('shop.openWeb')}
          </button>
        </div>
      )}
    </div>
  );
}
