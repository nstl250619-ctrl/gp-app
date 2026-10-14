// IDE 平台选择卡：七张纯图标卡（等分不溢出），状态用角标圆点 + 悬停提示。
import type { DetectResult } from '@/types/ipc';
import { t } from '@/i18n';
import workbuddyIcon from '@/assets/tools/workbuddy.png';
import codebuddyIcon from '@/assets/tools/codebuddy.png';
import dshIcon from '@/assets/tools/dsh.png';
import codexIcon from '@/assets/tools/codex.png';
import claudeIcon from '@/assets/tools/claude.png';
import openclawIcon from '@/assets/tools/openclaw.png';
import hermesIcon from '@/assets/tools/hermes.png';

export type TabKey = 'workbuddy' | 'codebuddy' | 'dsh' | 'codex' | 'claude' | 'openclaw' | 'hermes';
export const TABS: TabKey[] = ['workbuddy', 'codebuddy', 'dsh', 'codex', 'claude', 'openclaw', 'hermes'];
export const WRITABLE = new Set<TabKey>(TABS);
export const SINGLE_MODEL = new Set<TabKey>(['codex', 'claude']);

const ICONS: Record<TabKey, string> = {
  workbuddy: workbuddyIcon,
  codebuddy: codebuddyIcon,
  dsh: dshIcon,
  codex: codexIcon,
  claude: claudeIcon,
  openclaw: openclawIcon,
  hermes: hermesIcon,
};

// 名称文案：i18n 门禁行数限制，DSH 名放在 zhExtra.configExtra，其余在 index.ts config 段。
const NAME_KEYS: Record<TabKey, string> = {
  workbuddy: 'config.tabWorkbuddy',
  codebuddy: 'config.tabCodebuddy',
  dsh: 'configExtra.tabDsh',
  codex: 'config.tabCodex',
  claude: 'config.tabClaude',
  openclaw: 'config.tabOpenclaw',
  hermes: 'config.tabHermes',
};

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
        const configured = d?.hasManagedEntry ?? false;
        const found = d?.detected ?? false;
        // dot: green=configured, brand=found, gray=not located
        const dotCls = configured ? 'ok' : found ? 'found' : 'none';
        const tip = `${t(NAME_KEYS[key])} · ${
          configured
            ? `${t('config.found')} · ${t('config.ideConfigured')}`
            : found
              ? t('config.found')
              : t('config.notLocated')
        }`;
        return (
          <button
            key={key}
            type="button"
            className={`ide-tab ide-tab--icon${tab === key ? ' ide-tab--on' : ''}`}
            title={tip}
            onClick={() => onPick(key)}
          >
            <span className={`ide-tab__dot ide-tab__dot--${dotCls}`} />
            <img className="ide-tab__logo-img" src={ICONS[key]} alt={tip} draggable={false} />
          </button>
        );
      })}
    </div>
  );
}
