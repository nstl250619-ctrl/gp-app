// IDE 平台选择卡（§八 设计稿同款）：四张卡（缩写徽标 + 名称 + 状态行），待开发带角标。
import type { DetectResult } from '@/types/ipc';
import { t } from '@/i18n';

export type TabKey = 'workbuddy' | 'codebuddy' | 'openclaw' | 'hermes';
export const TABS: TabKey[] = ['workbuddy', 'codebuddy', 'openclaw', 'hermes'];
export const WRITABLE = new Set<TabKey>(['workbuddy', 'codebuddy']);

const ABBR: Record<TabKey, string> = { workbuddy: 'WB', codebuddy: 'CB', openclaw: 'OC', hermes: 'HM' };

interface Props {
  tab: TabKey;
  detectAll: DetectResult[];
  onPick: (k: TabKey) => void;
}

export default function IdeTabs({ tab, detectAll, onPick }: Props) {
  return (
    <div className="ide-tabs">
      {TABS.map((key) => {
        const d = detectAll.find((x) => x.target === key);
        const writable = WRITABLE.has(key);
        const st = !writable
          ? { cls: 'dim', text: t('config.soonSupport') }
          : d?.detected
            ? { cls: 'ok', text: d.hasManagedEntry ? `${t('config.found')} · ${t('config.ideConfigured')}` : t('config.found') }
            : { cls: 'dim', text: t('config.notLocated') };
        return (
          <button
            key={key}
            type="button"
            className={`ide-tab${tab === key ? ' ide-tab--on' : ''}`}
            onClick={() => onPick(key)}
          >
            {!writable && <span className="ide-tab__soon">{t('config.soonDev')}</span>}
            <span className="ide-tab__name">
              <span className="ide-tab__logo">{ABBR[key]}</span>
              {t(`config.tab${key.charAt(0).toUpperCase()}${key.slice(1)}`)}
            </span>
            <span className={`ide-tab__st ide-tab__st--${st.cls}`}>{st.text}</span>
          </button>
        );
      })}
    </div>
  );
}
