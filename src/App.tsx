import { useCallback, useEffect, useState } from 'react';
import { History, Settings as SettingsIcon, Store, Ticket, Wand2 } from 'lucide-react';
import { Gauge } from 'lucide-react';
import { t } from './i18n';
import { invokeCommand } from './services/ipc';
import type { AppStatus, InstanceCapability } from './types/ipc';
import { checkForUpdate, downloadAndInstall, isAutoUpdateOn } from './utils/updater';
import LoginScreen from './modules/auth/LoginScreen';
import LoginBackdrop from './modules/auth/LoginBackdrop';
import TopBar from './modules/layout/TopBar';
import RedeemPage from './modules/redeem/RedeemPage';
import ConfigPage from './modules/config/ConfigPage';
import UsagePage from './modules/config/UsagePage';
import ShopPage from './modules/shop/ShopPage';
import RecordsPage from './modules/records/RecordsPage';
import SettingsPage from './modules/settings/SettingsPage';

// 设计稿导航（§八）：一键配置 / 用量 / 商城 / 兑换 / 记录 / 设置
const PAGES = {
  config: ConfigPage,
  usage: UsagePage,
  shop: ShopPage,
  redeem: RedeemPage,
  records: RecordsPage,
  settings: SettingsPage,
} as const;

type PageKey = keyof typeof PAGES;

const MAIN_NAV: PageKey[] = ['config', 'usage', 'shop', 'redeem'];
const BOTTOM_NAV: PageKey[] = ['records', 'settings'];

const NAV_ICONS: Record<PageKey, typeof Gauge> = {
  config: Wand2,
  usage: Gauge,
  shop: Store,
  redeem: Ticket,
  records: History,
  settings: SettingsIcon,
};

const EMPTY_STATUS: AppStatus = {
  name: '',
  abbr: '',
  version: '',
  loggedIn: false,
  username: '',
  hasApiKey: false,
  targetDetected: false,
  shopReady: false,
};

export default function App() {
  const [page, setPage] = useState<PageKey>('config');
  // 页面保活：首次访问挂载，之后仅切换可见性（切页瞬时、状态不丢）
  const [visited, setVisited] = useState<Set<PageKey>>(() => new Set<PageKey>(['config']));
  const [probe, setProbe] = useState<InstanceCapability | null>(null);
  const [version, setVersion] = useState('');
  // 登录态：null=检测中，否则以 loggedIn 分流（§16 登录门面）
  const [auth, setAuth] = useState<AppStatus | null>(null);

  const refreshProbe = useCallback(() => {
    invokeCommand<InstanceCapability>('instance.probe', {}).then(setProbe).catch(() => {});
  }, []);

  // 持续探测循环：启动瞬间探测失败（网络抖动/DNS/TLS 慢）会永远显示「不可达」，
  // 失败后 15s 自动重试直至恢复；成功后降频 5min 保活探测。状态栏点击可立即重试。
  useEffect(() => {
    let cancelled = false;
    let timer: ReturnType<typeof setTimeout> | undefined;
    const run = async () => {
      try {
        const cap = await invokeCommand<InstanceCapability>('instance.probe', {});
        if (cancelled) return;
        setProbe(cap);
        timer = setTimeout(run, cap.reachable ? 5 * 60 * 1000 : 15 * 1000);
      } catch {
        if (cancelled) return;
        timer = setTimeout(run, 15 * 1000);
      }
    };
    void run();
    return () => {
      cancelled = true;
      if (timer) clearTimeout(timer);
    };
  }, []);

  // 启动即探测登录态
  useEffect(() => {
    invokeCommand<AppStatus>('app.status', {})
      .then(setAuth)
      .catch(() => setAuth(EMPTY_STATUS));
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
          if (ok) {
            downloadAndInstall().catch(() => {
              window.alert(t('settingsExtra.updateFailed'));
            });
          }
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

  // 登录态分流（§16）：检测中 → 闪屏；未登录 → 登录门面；已登录 → 主界面
  if (!auth) {
    return (
      <div className="splash">
        <LoginBackdrop />
        <div className="splash__inner">
          <span className="splash__logo">GP</span>
          <span className="splash__name">{t('nav.appTitle')} · GreenPool</span>
        </div>
      </div>
    );
  }

  if (!auth.loggedIn) {
    return <LoginScreen onDone={(username) => setAuth({ ...auth, loggedIn: true, username })} />;
  }

  return (
    <div className="app-shell">
      <TopBar username={auth.username} newapiOk={probe?.reachable} shopReady={auth.shopReady} />

      <aside className="sidebar">
        <nav className="sidebar__nav">{renderNav(MAIN_NAV)}</nav>
        <div className="sidebar__spacer" />
        <div className="sidebar__divider" />
        <button
          type="button"
          className="sidebar__qq"
          title={t('sidebar.qqCopyHint')}
          onClick={() => navigator.clipboard.writeText('237525313').catch(() => {})}
        >
          QQ:237525313
        </button>
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
              className="page"
              style={{ display: active ? undefined : 'none' }}
            >
              <Comp onNavigate={navigate} />
            </div>
          );
        })}
      </main>

      <footer className="statusbar">
        <span
          style={{ display: 'inline-flex', alignItems: 'center', gap: 'var(--space-2)', cursor: 'pointer' }}
          onClick={refreshProbe}
          title={t('statusbar.probeRetry')}
        >
          <span className={`dot ${probeState === 'ok' ? 'dot--ok' : probeState === 'fail' ? 'dot--warn' : ''}`} />
          {probeText}
        </span>
        <span>v{version || '0.1.0'}</span>
      </footer>
    </div>
  );
}
