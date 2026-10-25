import { useCallback, useEffect, useState } from 'react';
import { History, Settings as SettingsIcon, Store, Ticket, Wand2, Zap } from 'lucide-react';
import { Gauge } from 'lucide-react';
import { listen } from '@tauri-apps/api/event';
import { t } from './i18n';
import { invokeCommand } from './services/ipc';
import type { AppStatus, InstanceCapability, TunnelStatus } from './types/ipc';
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

/// 直链加速总开关（产品决策）：false = 功能暂不启用（启动/停止禁用、SSH 凭据为占位），
/// 确认上线时改 true 并恢复 secrets.rs / tunnel.rs / secrets.enc 的真实凭据。
const TURBO_ENABLED = true;

/// 统一取错误文案（Tauri 命令错误可能是字符串或 {message} 对象）。
function toMessage(e: unknown): string {
  if (typeof e === 'string') return e;
  if (e && typeof e === 'object' && 'message' in e) return String((e as { message: unknown }).message);
  return String(e);
}

/// 直链加速卡片（会员功能公告卡）：无独立页面，侧栏公告式卡片。
/// 状态映射：stopped→启动(可点) / connecting→连接中 / reconnecting→重试中 /
/// connected|degraded→运行中 / error→重试(可点，start 在 error 态语义为取消旧轮次重启)。
function TurboCard({ tunnel, group, onStart, onStop, error }: {
  tunnel: TunnelStatus | null;
  group: string | null;
  onStart: () => void;
  onStop: () => void;
  error: string | null;
}) {
  const phase = tunnel?.phase ?? 'stopped';
  // 会员判定（产品决策）：仅 svip 分组可启动；分组未知按非会员处理
  const isSvip = group === 'svip';
  const hint = !TURBO_ENABLED ? t('turbo.comingSoon') : !isSvip ? t('turbo.svipOnly') : undefined;
  const canStart = TURBO_ENABLED && isSvip && (phase === 'stopped' || phase === 'error');
  const startLabel =
    phase === 'connected' || phase === 'degraded'
      ? t('turbo.running')
      : phase === 'connecting' || phase === 'reconnecting'
        ? t('turbo.connecting')
        : phase === 'error'
          ? t('turbo.retry')
          : t('turbo.start');
  const statusLabel =
    phase === 'connected' || phase === 'degraded'
      ? t('turbo.statusRunning')
      : phase === 'connecting' || phase === 'reconnecting'
        ? t('turbo.statusConnecting')
        : phase === 'error'
          ? t('turbo.statusError')
          : t('turbo.statusStopped');
  const dotClass =
    phase === 'connected'
      ? 'dot dot--ok'
      : phase === 'degraded' || phase === 'connecting' || phase === 'reconnecting'
        ? 'dot dot--connecting'
        : phase === 'error'
          ? 'dot dot--danger'
          : 'dot';
  return (
    <div className="turbo-card" title={tunnel?.message || ''}>
      <div className="turbo-card__head">
        <Zap size={14} className="turbo-card__icon" />
        <span className="turbo-card__title">{t('turbo.title')}</span>
        <span className="turbo-card__badge">{t('turbo.badge')}</span>
      </div>
      <ul className="turbo-card__features">
        {[t('turbo.feature1'), t('turbo.feature2'), t('turbo.feature3'), t('turbo.feature4'), t('turbo.feature5')].map((f) => (
          <li key={f}>{f}</li>
        ))}
      </ul>
      <div className="turbo-card__actions">
        <button
          type="button"
          className="turbo-btn turbo-btn--primary"
          disabled={!canStart}
          title={hint}
          onClick={onStart}
        >
          {startLabel}
        </button>
        <button
          type="button"
          className="turbo-btn"
          disabled={!TURBO_ENABLED || phase === 'stopped'}
          title={hint}
          onClick={onStop}
        >
          {t('turbo.stop')}
        </button>
      </div>
      <div className="turbo-card__status">
        <span className={dotClass} />
        <span>{statusLabel}</span>
      </div>
      {error && (
        <div className="hint" style={{ color: 'var(--danger)', marginTop: 'var(--space-1)', fontSize: 12 }}>
          {error}
        </div>
      )}
    </div>
  );
}

export default function App() {
  const [page, setPage] = useState<PageKey>('config');
  // 页面保活：首次访问挂载，之后仅切换可见性（切页瞬时、状态不丢）
  const [visited, setVisited] = useState<Set<PageKey>>(() => new Set<PageKey>(['config']));
  const [probe, setProbe] = useState<InstanceCapability | null>(null);
  const [version, setVersion] = useState('');
  // 登录态：null=检测中，否则以 loggedIn 分流（§16 登录门面）
  const [auth, setAuth] = useState<AppStatus | null>(null);
  // 隧道状态（底部导航启动/停止控件用）：事件驱动，无独立页面
  const [tunnel, setTunnel] = useState<TunnelStatus | null>(null);
  // 隧道操作错误（启动/停止失败时显示在卡片上，不再静默吞掉）
  const [tunnelError, setTunnelError] = useState<string | null>(null);

  useEffect(() => {
    invokeCommand<TunnelStatus>('tunnel.status', {}).then(setTunnel).catch(() => {});
    let disposed = false;
    let off: (() => void) | undefined;
    listen<TunnelStatus>('tunnel://status', (e) => {
      if (!disposed) setTunnel(e.payload);
    })
      .then((fn) => {
        if (disposed) fn();
        else off = fn;
      })
      .catch(() => {});
    return () => {
      disposed = true;
      off?.();
    };
  }, []);

  // 隧道操作：错误不再吞（用户点「启动」无感就是这里吞了错误）+ 主动拉状态兜底事件延迟/丢失
  const refreshTunnel = () => {
    invokeCommand<TunnelStatus>('tunnel.status', {}).then(setTunnel).catch(() => {});
  };
  const startTunnel = async () => {
    setTunnelError(null);
    try {
      await invokeCommand('tunnel.start', {});
    } catch (e) {
      setTunnelError(toMessage(e));
    }
    // 立即 + 延迟刷新：ssh 起来后状态要过一两秒才变 connected，事件也可能丢
    refreshTunnel();
    setTimeout(refreshTunnel, 1500);
    setTimeout(refreshTunnel, 4000);
  };
  const stopTunnel = async () => {
    setTunnelError(null);
    try {
      await invokeCommand('tunnel.stop', {});
    } catch (e) {
      setTunnelError(toMessage(e));
    }
    refreshTunnel();
    setTimeout(refreshTunnel, 800);
  };

  // 存量升级用户补 DSH 专属 key 明文（幂等）：v0.3.0 前未写过钥匙串条目，
  // 升级后不重新登录也要就绪；未登录时后端静默失败，登录流程会再补。
  useEffect(() => {
    invokeCommand('account.setupDshKey', {}).catch(() => {});
  }, []);

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

  // 认证态刷新（H2）：仅窗口聚焦时刷新，30 秒节流。
  // 不做定时轮询——会话失效时轮询会反复触发静默重登，撞上 Shop 登录限流（同账号 10 次/分钟）。
  useEffect(() => {
    let last = 0;
    const refreshAuth = () => {
      const now = Date.now();
      if (now - last < 30_000) return;
      last = now;
      invokeCommand<AppStatus>('app.status', {})
        .then(setAuth)
        .catch(() => {});
    };
    window.addEventListener('focus', refreshAuth);
    return () => window.removeEventListener('focus', refreshAuth);
  }, []);

  useEffect(() => {
    // 版本号 + 自动更新检测（instance.probe 已由上方探测循环负责，不在此重复触发）
    import('@tauri-apps/api/app').then(({ getVersion }) =>
      getVersion().then(setVersion).catch(() => {}),
    );
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
  }, []);

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
        <TurboCard tunnel={tunnel} group={auth?.group ?? null} onStart={startTunnel} onStop={stopTunnel} error={tunnelError} />
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
