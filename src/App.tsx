import { useCallback, useEffect, useState } from 'react';
import { History, Link2, Settings as SettingsIcon, Store, Ticket, Wand2, Wallet } from 'lucide-react';
import { Gauge } from 'lucide-react';
import { t } from './i18n';
import { invokeCommand } from './services/ipc';
import type { InstanceCapability } from './types/ipc';
import { checkForUpdate, downloadAndInstall, isAutoUpdateOn } from './utils/updater';
import OverviewPage from './modules/overview/OverviewPage';
import RedeemPage from './modules/redeem/RedeemPage';
import ConfigPage from './modules/config/ConfigPage';
import ShopPage from './modules/shop/ShopPage';
import WalletPage from './modules/wallet/WalletPage';
import RecordsPage from './modules/records/RecordsPage';
import SettingsPage from './modules/settings/SettingsPage';

const PAGES = {
  overview: OverviewPage,
  redeem: RedeemPage,
  config: ConfigPage,
  shop: ShopPage,
  wallet: WalletPage,
  records: RecordsPage,
  settings: SettingsPage,
} as const;

type PageKey = keyof typeof PAGES;

const MAIN_NAV: PageKey[] = ['overview', 'redeem', 'config', 'shop', 'wallet'];
const BOTTOM_NAV: PageKey[] = ['records', 'settings'];

const NAV_ICONS: Record<PageKey, typeof Gauge> = {
  overview: Gauge,
  redeem: Ticket,
  config: Wand2,
  shop: Store,
  wallet: Wallet,
  records: History,
  settings: SettingsIcon,
};

export default function App() {
  const [page, setPage] = useState<PageKey>('overview');
  // 页面保活：首次访问挂载，之后仅切换可见性（切页瞬时、状态/iframe 不丢）
  const [visited, setVisited] = useState<Set<PageKey>>(() => new Set<PageKey>(['overview']));
  const [probe, setProbe] = useState<InstanceCapability | null>(null);
  const [version, setVersion] = useState('');

  const refreshProbe = useCallback(() => {
    invokeCommand<InstanceCapability>('instance.probe', {}).then(setProbe).catch(() => {});
  }, []);

  useEffect(() => {
    refreshProbe();
    import('@tauri-apps/api/app').then(({ getVersion }) =>
      getVersion().then(setVersion).catch(() => {}),
    );
    // 自动更新（仅开关开启时检测；发现新版本仍需用户确认后才下载安装）
    if (isAutoUpdateOn()) {
      checkForUpdate()
        .then((u) => {
          if (!u) return;
          const ok = window.confirm(
            `${t('nav.appTitle')} v${u.version}：${t('settings.available')}，${t('settings.updateNow')}？`,
          );
          if (ok) downloadAndInstall().catch(() => {});
        })
        .catch(() => {});
    }
  }, [refreshProbe]);

  useEffect(() => {
    setVisited((v) => (v.has(page) ? v : new Set(v).add(page)));
  }, [page]);

  const navigate = useCallback((p: string) => {
    setPage(p as PageKey);
  }, []);

  const renderNav = (keys: PageKey[]) =>
    keys.map((key) => {
      const Icon = NAV_ICONS[key];
      return (
        <button
          key={key}
          type="button"
          className={page === key ? 'nav-item nav-item--active' : 'nav-item'}
          onClick={() => setPage(key)}
        >
          <Icon size={18} />
          {t(`nav.${key}`)}
        </button>
      );
    });

  const probeState = !probe ? 'unknown' : probe.reachable ? 'ok' : 'fail';
  const probeText =
    probeState === 'ok' ? t('statusbar.probeOk') : probeState === 'fail' ? t('statusbar.probeFail') : t('statusbar.probeUnknown');

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <div className="sidebar__brand">
          <Link2 size={20} color="var(--accent)" />
          {t('nav.appTitle')}
        </div>
        <nav className="sidebar__nav">{renderNav(MAIN_NAV)}</nav>
        <div className="sidebar__spacer" />
        <div className="sidebar__divider" />
        <nav className="sidebar__nav" style={{ paddingBottom: 'var(--space-3)' }}>{renderNav(BOTTOM_NAV)}</nav>
      </aside>

      <main className="content">
        {(Object.keys(PAGES) as PageKey[]).map((key) => {
          const Comp = PAGES[key];
          const active = page === key;
          if (!visited.has(key)) return null;
          return (
            <div
              key={key}
              className={key === 'shop' ? 'page page--full' : 'page'}
              style={{ display: active ? undefined : 'none' }}
            >
              <Comp onNavigate={navigate} />
            </div>
          );
        })}
      </main>

      <footer className="statusbar">
        <span style={{ display: 'inline-flex', alignItems: 'center', gap: 'var(--space-2)' }}>
          <span className={`dot ${probeState === 'ok' ? 'dot--ok' : probeState === 'fail' ? 'dot--warn' : ''}`} />
          {probeText}
        </span>
        <span>v{version || '0.1.0'}</span>
      </footer>
    </div>
  );
}
