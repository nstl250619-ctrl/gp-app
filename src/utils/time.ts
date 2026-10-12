// 相对时间格式化与轻量校验。日期文案来自 i18n，具体数字格式交给 Intl（zh-CN locale 数据）。
import { t } from '@/i18n';

const hmFormatter = new Intl.DateTimeFormat('zh-CN', { hour: '2-digit', minute: '2-digit', hour12: false });
const mdFormatter = new Intl.DateTimeFormat('zh-CN', { month: 'long', day: 'numeric' });
const dateTimeFormatter = new Intl.DateTimeFormat('zh-CN', {
  year: 'numeric',
  month: '2-digit',
  day: '2-digit',
  hour: '2-digit',
  minute: '2-digit',
  hour12: false,
});

function sameDay(a: Date, b: Date): boolean {
  return a.getFullYear() === b.getFullYear() && a.getMonth() === b.getMonth() && a.getDate() === b.getDate();
}

export function isValidEmail(v: string): boolean {
  const s = v.trim();
  return s.includes('@') && !s.startsWith('@') && !s.endsWith('@') && !s.includes('@', s.indexOf('@') + 1);
}

export function formatRelative(date: Date, now: Date = new Date()): string {
  const diffMin = Math.floor((now.getTime() - date.getTime()) / 60000);
  if (diffMin < 1) return t('time.justNow');
  if (diffMin < 60) return `${diffMin} ${t('time.minutesAgo')}`;
  const hours = Math.floor(diffMin / 60);
  if (hours < 24) return `${hours} ${t('time.hoursAgo')}`;

  const hm = hmFormatter.format(date);
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (sameDay(date, now)) return `${t('time.today')} ${hm}`;
  if (sameDay(date, yesterday)) return `${t('time.yesterday')} ${hm}`;
  return `${mdFormatter.format(date)} ${hm}`;
}

export function formatUnix(seconds: number, now: Date = new Date()): string {
  if (!seconds) return '';
  return formatRelative(new Date(seconds * 1000), now);
}

/// 绝对时间点（订阅到期、兑换码有效期等未来时间），输出固定日期时间（如 2026/10/14 01:23）。
/// 不能用 formatRelative：其对未来时间会得到负 diffMin 而误显示成「刚刚」。
export function formatDateTime(seconds: number): string {
  if (!seconds) return '';
  return dateTimeFormatter.format(new Date(seconds * 1000));
}
