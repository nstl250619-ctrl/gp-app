// 顶栏（§16 主界面重构）：品牌 logo 图 + 服务徽章 + 账号邮箱 + 头像。
import { t } from '@/i18n';
import brandLogo from '@/assets/brand-logo.png';

interface Props {
  username: string;
  newapiOk?: boolean;
  shopReady?: boolean;
}

export default function TopBar({ username, newapiOk, shopReady }: Props) {
  const initial = username.trim().charAt(0).toUpperCase() || 'G';

  return (
    <header className="topbar">
      <img src={brandLogo} alt="GreenPool" className="topbar__logo" />
      <span className="topbar__name">{t('nav.appTitle')}</span>
      <div className="topbar__badges">
        <span className="topbar__badge">
          <span className={`dot ${newapiOk ? 'dot--ok' : 'dot--warn'}`} />
          {t('topbar.badgeNewapi')}
        </span>
        <span className="topbar__badge">
          <span className={`dot ${shopReady ? 'dot--ok' : 'dot--warn'}`} />
          {t('nav.shop')}
        </span>
      </div>
      <div className="topbar__right">
        <span className="topbar__acct">
          <span className="topbar__mail">{username}</span>
          <span className="topbar__role">{t('topbar.role')}</span>
        </span>
        <span className="topbar__ava">{initial}</span>
      </div>
    </header>
  );
}
