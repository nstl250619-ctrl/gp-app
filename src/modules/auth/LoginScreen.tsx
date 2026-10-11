// 全屏登录门面（§16）：深色沉浸背景 + 居中毛玻璃卡片，登录成功后三段消退过渡到主界面。
import { useState } from 'react';
import { t } from '@/i18n';
import LoginBackdrop from './LoginBackdrop';
import LoginForm, { type AuthMode } from './LoginForm';

interface Props {
  onDone: (username: string) => void;
}

export default function LoginScreen({ onDone }: Props) {
  const [mode, setMode] = useState<AuthMode>('login');
  const [leavingCard, setLeavingCard] = useState(false);
  const [leavingScreen, setLeavingScreen] = useState(false);

  // 三段消退：卡片先缩放淡出 → 整层背景缓慢消退 → 通知父组件切主界面
  const startLeave = (username: string) => {
    setLeavingCard(true);
    window.setTimeout(() => setLeavingScreen(true), 420);
    window.setTimeout(() => onDone(username), 880);
  };

  const screenClass =
    'login-screen' +
    (leavingCard ? ' login-screen--leaving-card' : '') +
    (leavingScreen ? ' login-screen--leaving' : '');

  return (
    <div className={screenClass}>
      <LoginBackdrop />

      <div className="login-card">
        <div className="login-card__brand">
          <span className="login-card__logo">GP</span>
          <span className="login-card__name">{t('nav.appTitle')}</span>
          <span className="login-card__tag">{t('login.brandTag')}</span>
        </div>
        <div className="login-card__sub">{t('login.sub')}</div>

        <div className="login-card__tabs">
          <button
            type="button"
            className={`login-card__tab${mode === 'login' ? ' login-card__tab--on' : ''}`}
            onClick={() => setMode('login')}
          >
            {t('settings.modeLogin')}
          </button>
          <button
            type="button"
            className={`login-card__tab${mode === 'register' ? ' login-card__tab--on' : ''}`}
            onClick={() => setMode('register')}
          >
            {t('settings.modeRegister')}
          </button>
        </div>

        <LoginForm mode={mode} onSwitchMode={setMode} onSuccess={startLeave} />
      </div>
    </div>
  );
}
