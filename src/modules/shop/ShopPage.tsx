// 商城页（§八 + P3 网关直购）：余额条 + 动态商品卡（Shop 在售，余额直购发货）+ 常驻尝鲜引流卡 + 最近订单。
// 修复：shop.status 加载中（null）误显「尚未开放」；iframe 已彻底移除。
import { useEffect, useState } from 'react';
import { ExternalLink, Gem, Ticket, Zap } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { BuyResult, ProductItem, ShopOrderItem, ShopStatus, WalletSummary } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';

export default function ShopPage(_props: PageProps) {
  const [shop, setShop] = useState<ShopStatus | null>(null);
  const [wallet, setWallet] = useState<WalletSummary | null>(null);
  const [goods, setGoods] = useState<ProductItem[] | null>(null);
  const [goodsErr, setGoodsErr] = useState<string | null>(null);
  const [orders, setOrders] = useState<ShopOrderItem[] | null>(null);
  const [busyId, setBusyId] = useState<number | null>(null);
  const [buyMsg, setBuyMsg] = useState<{ ok: boolean; text: string } | null>(null);

  const loadGoods = () => {
    setGoodsErr(null);
    invokeCommand<ProductItem[]>('shop.products', {})
      .then(setGoods)
      .catch((e) => {
        // 错误透传：不再静默清空（吞错会让「商品不显示」无从排查）
        setGoods([]);
        setGoodsErr(errorMessage(e));
      });
  };

  useEffect(() => {
    invokeCommand<ShopStatus>('shop.status', {})
      .then(setShop)
      .catch(() => setShop({ ready: false, baseUrl: '', ssoReady: false, message: '' }));
    invokeCommand<WalletSummary>('shop.wallet', {}).then(setWallet).catch(() => {});
    loadGoods();
    invokeCommand<ShopOrderItem[]>('shop.my_orders', {}).then(setOrders).catch(() => setOrders([]));
  }, []);

  const openShop = async () => {
    const u = shop?.baseUrl || 'https://shop.greenpool.cn';
    await invokeCommand('app.openExternal', { url: u }).catch(() => {});
  };

  const buy = async (g: ProductItem) => {
    setBusyId(g.id);
    setBuyMsg(null);
    try {
      const d = await invokeCommand<{ skus: { id: number }[] }>('shop.product_detail', { id: g.id });
      const sku = d.skus?.[0];
      if (!sku) throw new Error(t('mall.goodsEmpty'));
      const r = await invokeCommand<BuyResult>('shop.buy_order', { goodsId: g.id, skuId: sku.id });
      setBuyMsg({ ok: true, text: `${t('mall.buyDone')}${r.deliveryText}` });
      invokeCommand<WalletSummary>('shop.wallet', {}).then(setWallet).catch(() => {});
      invokeCommand<ShopOrderItem[]>('shop.my_orders', {}).then(setOrders).catch(() => {});
    } catch (e) {
      setBuyMsg({ ok: false, text: `${t('mall.buyFail')}：${errorMessage(e)}` });
    } finally {
      setBusyId(null);
    }
  };

  return (
    <div>
      <h1>{t('nav.shop')}</h1>
      <div className="sub">{t('mall.goodsTitle')}</div>

      {shop === null ? (
        <p className="muted">{t('common.loading')}</p>
      ) : !shop.ready ? (
        <div className="shop-frame">
          <div className="card__big">{t('shop.notReady')}</div>
          <div className="card__desc">{shop.message || t('shop.notReadyDesc')}</div>
          <button type="button" className="btn btn-primary" style={{ marginTop: 'var(--space-3)' }} onClick={openShop}>
            <ExternalLink size={15} />
            {t('shop.openWeb')}
          </button>
        </div>
      ) : (
        <>
          <div className="balbar">
            <span className="balbar__t">{t('mall.balanceLabel')}</span>
            <span className="balbar__a">{wallet ? `$ ${wallet.balance}` : '—'}</span>
          </div>

          {buyMsg && (
            <div className={`apply-banner ${buyMsg.ok ? 'apply-banner--ok' : 'apply-banner--fail'}`}>{buyMsg.text}</div>
          )}

          {/* trial teaser card: displayed always, sold on other channels */}
          <div className="mall-goods">
            <div className="pcard">
              <div className="pcard__img"><Gem size={30} color="var(--brand-green)" /></div>
              <div className="pcard__bd">
                <div className="pcard__nm">{t('mall.trialTitle')}</div>
                <div className="pcard__dc">{t('mall.trialDesc')}</div>
                <div className="pcard__rw">
                  <span className="pcard__pr">{t('mall.trialPrice')}</span>
                  <span className="muted small" style={{ marginLeft: 'auto' }}>{t('mall.trialCta')}</span>
                </div>
              </div>
            </div>
            {goodsErr && (
            <div className="apply-banner apply-banner--fail" style={{ marginTop: 'var(--space-3)' }}>
              {goodsErr}
              <button type="button" className="btn btn--ghost" style={{ marginLeft: 'auto' }} onClick={loadGoods}>
                {t('common.retry')}
              </button>
            </div>
          )}
          {(goods || []).map((g) => (
              <div className="pcard" key={g.id}>
                <div className="pcard__img">
                  {g.fulfillType === 'content' ? <Ticket size={30} color="var(--accent)" /> : <Zap size={30} color="var(--accent)" />}
                </div>
                <div className="pcard__bd">
                  <div className="pcard__nm">
                    {g.title}
                    {g.id === 8 && <span className="pcard__tag">{t('mall.recommend')}</span>}
                  </div>
                  <div className="pcard__dc">{g.description}</div>
                  <div className="pcard__rw">
                    <span className="pcard__pr">${Number(g.price).toFixed(2)}</span>
                    <button
                      type="button"
                      className="btn"
                      style={{ marginLeft: 'auto' }}
                      disabled={busyId === g.id}
                      onClick={() => buy(g)}
                    >
                      {busyId === g.id ? t('mall.buying') : t('mall.buy')}
                    </button>
                  </div>
                </div>
              </div>
            ))}
          </div>
          {!goodsErr && goods !== null && goods.length === 0 && <p className="muted" style={{ marginTop: 'var(--space-2)' }}>{t('mall.goodsEmpty')}</p>}
          <div className="hint" style={{ marginTop: 'var(--space-2)' }}>{t('mall.trialNote')}</div>

          <div className="card" style={{ marginTop: 'var(--space-4)' }}>
            <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>{t('mall.ordersTitle')}</div>
            {orders && orders.length === 0 && <p className="muted">{t('mall.ordersEmpty')}</p>}
            {orders?.map((o) => (
              <div className="rec" key={o.orderId}>
                <span>
                  {o.productName}
                  {o.codes?.length > 0 && (
                    <span className="muted small" style={{ marginLeft: 8 }}>· {o.codes.map((c: { code: string }) => c.code).join(' / ')}</span>
                  )}
                </span>
                <span className="muted" style={{ fontSize: 11.5 }}>${o.amount} · {o.status}</span>
              </div>
            ))}
            {!orders && <p className="muted">{t('common.loading')}</p>}
          </div>
        </>
      )}
    </div>
  );
}
