// 商城交互偏好（本地持久化）：
// 商品卡点击是否跳转商城详情页，默认关闭（设置页可开）。
const SHOP_CARD_LINK_KEY = 'gp.shopCardLink';

export function isShopCardLinkOn(): boolean {
  return localStorage.getItem(SHOP_CARD_LINK_KEY) === 'on';
}

export function setShopCardLink(on: boolean): void {
  localStorage.setItem(SHOP_CARD_LINK_KEY, on ? 'on' : 'off');
}
