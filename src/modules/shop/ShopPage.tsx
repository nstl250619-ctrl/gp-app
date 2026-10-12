// 商城页（§八 + P3 网关直购）：静态商品卡（秒开）+ 余额条 + 常驻尝鲜引流卡 + 最近订单。
// 商品卡不依赖 Shop 商品列表回传；购买时才按稳定 key 调网关下单（云端扣款发货）。
import { useEffect, useState } from 'react';
import { ExternalLink, Gem, Ticket } from 'lucide-react';
import { errorMessage, invokeCommand } from '@/services/ipc';
import type { BuyResult, ShopOrderItem, ShopStatus, WalletSummary } from '@/types/ipc';
import type { PageProps } from '@/types/page';
import { t } from '@/i18n';
import RechargeModal from './RechargeModal';

// 静态可购商品卡：key 为网关下单的稳定标识；price 仅作展示，购买成功后按成交价校准。
// goodsId 对应商城商品（点击卡片跳转商城详情页 /mall/{goodsId}，SSO 自动登录）。
const STATIC_GOODS = [
  { key: 'daily', goodsId: 9, titleKey: 'mall.dailyTitle', price: '0.29', featKey: 'mall.feat9', recommend: false },
  { key: 'weekly', goodsId: 7, titleKey: 'mall.weeklyTitle', price: '0.29', featKey: 'mall.feat7', recommend: false },
  { key: 'monthly', goodsId: 8, titleKey: 'mall.monthlyTitle', price: '1.42', featKey: 'mall.feat8', recommend: true },
] as const;

// 自动激活的订阅套餐 id → 一键购买成功文案 key
const BUY_PLAN_LABEL: Record<number, string> = {
  2: 'mall.buySub2',
  3: 'mall.buySub3',
  4: 'mall.buySub4',
  5: 'mall.buySub5',
};

function FeatureLines({ text }: { text: string }) {
  return (
    <div className="pcard__ft">
      {text.split('\n').filter(Boolean).map((line) => (
        <div className="pcard__ft-row" key={line}>
          <span className="pcard__ft-dot">·</span>
          <span>{line}</span>
        </div>
      ))}
    </div>
  );
}

export default function ShopPage(_props: PageProps) {
  const [shop, setShop] = useState<ShopStatus | null>(null);
  const [wallet, setWallet] = useState<WalletSummary | null>(null);
  const [orders, setOrders] = useState<ShopOrderItem[] | null>(null);
  const [prices, setPrices] = useState<Record<string, string>>(() =>
    Object.fromEntries(STATIC_GOODS.map((g) => [g.key, g.price])),
  );
  const [busyKey, setBusyKey] = useState<string | null>(null);
  const [buyMsg, setBuyMsg] = useState<{ ok: boolean; text: string; code?: string } | null>(null);
  const [rechargeOpen, setRechargeOpen] = useState(false);

  useEffect(() => {
    // 商品卡为静态渲染（不阻塞首屏）；这里只拉余额/状态/订单
    invokeCommand<ShopStatus>('shop.status', {})
      .then(setShop)
      .catch(() => setShop({ ready: false, baseUrl: '', ssoReady: false, message: '' }));
    invokeCommand<WalletSummary>('shop.wallet', {}).then(setWallet).catch(() => {});
    invokeCommand<ShopOrderItem[]>('shop.my_orders', {}).then(setOrders).catch(() => setOrders([]));
  }, []);

  const openShop = async () => {
    const u = shop?.baseUrl || 'https://shop.greenpool.cn';
    await invokeCommand('app.openExternal', { url: u }).catch(() => {});
  };

  // 点击商品卡 → SSO 自动登录后直达商城详情页（如 /mall/7）
  const openDetail = async (goodsId: number) => {
    try {
      const sso = await invokeCommand<string>('shop.ssoUrl', {});
      await invokeCommand('app.openExternal', { url: `${sso}&next=/mall/${goodsId}` });
    } catch { /* ignore */ }
  };

  const buy = async (key: string) => {
    setBusyKey(key);
    setBuyMsg(null);
    try {
      const r = await invokeCommand<BuyResult>('shop.buyOrder', { productKey: key });
      // 成交价回传校准（不阻碍本次成交）：用真实扣款金额刷新展示价
      if (r.amount > 0) setPrices((p) => ({ ...p, [key]: r.amount.toFixed(2) }));
      // 一键购买：网关已就地核销 + 自动订阅 → 直接显示订阅激活结果
      const planLabel = r.planId ? BUY_PLAN_LABEL[r.planId] : null;
      if (r.redeemed && planLabel) {
        setBuyMsg({ ok: true, text: t(planLabel), code: r.deliveryText });
      } else if (r.redeemed) {
        setBuyMsg({ ok: true, text: `${t('mall.buyRedeemed')}（+$${r.grantedUsd.toFixed(2)}）`, code: r.deliveryText });
      } else {
        setBuyMsg({ ok: true, text: t('mall.buyPending'), code: r.deliveryText });
      }
      invokeCommand<WalletSummary>('shop.wallet', {}).then(setWallet).catch(() => {});
      invokeCommand<ShopOrderItem[]>('shop.my_orders', {}).then(setOrders).catch(() => {});
      window.dispatchEvent(new CustomEvent('gp:balance-changed'));
    } catch (e) {
      setBuyMsg({ ok: false, text: `${t('mall.buyFail')}：${errorMessage(e)}` });
    } finally {
      setBusyKey(null);
    }
  };

  return (
    <div>
      <h1>{t('nav.shop')}</h1>
      <div className="sub">{t('mall.goodsTitle')}</div>

      {shop !== null && !shop.ready && (
        <div className="shop-frame" style={{ marginBottom: 'var(--space-3)' }}>
          <div className="card__big">{t('shop.notReady')}</div>
          <div className="card__desc">{shop.message || t('shop.notReadyDesc')}</div>
          <button type="button" className="btn btn-primary" style={{ marginTop: 'var(--space-3)' }} onClick={openShop}>
            <ExternalLink size={15} />
            {t('shop.openWeb')}
          </button>
        </div>
      )}

      <div className="balbar">
        <span className="balbar__t">{t('mall.balanceLabel')}</span>
        <button type="button" className="balbar__recharge" onClick={() => setRechargeOpen(true)}>
          {t('mall.rechargeBtn')}
        </button>
        <span className="balbar__a">{wallet ? `$ ${wallet.balance}` : '—'}</span>
      </div>

      {buyMsg && (
        <div className={`apply-banner ${buyMsg.ok ? 'apply-banner--ok' : 'apply-banner--fail'}`}>
          <div>{buyMsg.text}</div>
          {buyMsg.code && (
            <div className="rec__code" style={{ marginTop: 6, display: 'inline-flex' }}>
              <span className="rec__code-label">{t('mall.codeLabel')}</span>
              {buyMsg.code}
            </div>
          )}
        </div>
      )}

      <div className="mall-goods">
        {STATIC_GOODS.map((g) => (
          <div className="pcard pcard--link" key={g.key} onClick={() => openDetail(g.goodsId)}>
            <div className="pcard__img"><Ticket size={30} color="var(--accent)" /></div>
            <div className="pcard__bd">
              <div className="pcard__nm">
                {t(g.titleKey)}
                {g.recommend && <span className="pcard__tag">{t('mall.recommend')}</span>}
              </div>
              <FeatureLines text={t(g.featKey)} />
              <div className="pcard__rw">
                <span className="pcard__pr">${prices[g.key] ?? g.price}</span>
                <button
                  type="button"
                  className="btn"
                  style={{ marginLeft: 'auto' }}
                  disabled={busyKey === g.key}
                  onClick={(e) => { e.stopPropagation(); buy(g.key); }}
                >
                  {busyKey === g.key ? t('mall.buying') : t('mall.buy')}
                </button>
              </div>
            </div>
          </div>
        ))}

        {/* trial teaser card: displayed always, sold on other channels */}
        <div className="pcard">
          <div className="pcard__img"><Gem size={30} color="var(--brand-green)" /></div>
          <div className="pcard__bd">
            <div className="pcard__nm">{t('mall.trialTitle')}</div>
            <FeatureLines text={t('mall.trialFeatures')} />
            <div className="pcard__rw">
              <span className="pcard__pr">{t('mall.trialPrice')}</span>
              <span className="muted small" style={{ marginLeft: 'auto' }}>{t('mall.trialCta')}</span>
            </div>
          </div>
        </div>
      </div>

      <div className="hint" style={{ marginTop: 'var(--space-2)' }}>{t('mall.trialNote')}</div>

      <div className="card" style={{ marginTop: 'var(--space-4)' }}>
        <div style={{ fontWeight: 600, fontSize: 13, marginBottom: 4 }}>{t('mall.ordersTitle')}</div>
        {orders && orders.length === 0 && <p className="muted">{t('mall.ordersEmpty')}</p>}
        {orders?.map((o) => (
          <div className="rec" key={o.orderId}>
            <div className="rec__top">
              <span className="rec__name">{o.productName}</span>
              <span className="muted" style={{ fontSize: 11.5, flexShrink: 0 }}>${o.amount} · {o.status}</span>
            </div>
            {/* hide delivery content on refunded orders; truncate long content (e.g. JSON-file goods) */}
            {o.status !== 'refunded' && o.codes?.length > 0 && (
              <div className="rec__codes">
                {o.codes.map((c) => (
                  <span className="rec__code" key={c.codeId}>
                    <span className="rec__code-label">{t('mall.codeLabel')}</span>
                    {c.code.length > 48 ? `${c.code.slice(0, 48)}…` : c.code}
                  </span>
                ))}
              </div>
            )}
          </div>
        ))}
        {!orders && <p className="muted">{t('common.loading')}</p>}
      </div>

      {rechargeOpen && (
        <RechargeModal
          wallet={wallet}
          onClose={() => setRechargeOpen(false)}
          onPaid={() => {
            invokeCommand<WalletSummary>('shop.wallet', {}).then(setWallet).catch(() => {});
          }}
        />
      )}
    </div>
  );
}
