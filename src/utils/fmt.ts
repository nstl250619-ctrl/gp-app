// USD 格式化（与 new-api 计量口径一致：quota/500000 = USD）
// 去尾零：0.001470→$0.00147；0→$0；1.5→$1.5
export function fmtUsd(usd: number): string {
  return `$${parseFloat(usd.toFixed(6))}`;
}

// 积分口径（2026-10-09 产品裁决）：$1 = 1000 积分，固定兑换比例。
// 积分 = USD × 1000，向上保留 1 位小数（0.592 → 0.6）。
export function fmtCredits(usd: number): string {
  const c = Math.ceil(usd * 1000 * 10) / 10;
  return c.toFixed(1);
}

// 消耗积分：同上，但最低 0.1 积分（凡有消耗至少计 0.1）。
export function fmtCreditsSpent(usd: number): string {
  if (usd <= 0) return '0.0';
  const c = Math.max(0.1, Math.ceil(usd * 1000 * 10) / 10);
  return c.toFixed(1);
}
