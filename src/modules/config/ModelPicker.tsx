// 模型勾选列表：账号可用模型 + 全选/清空 + 已选计数。
import { t } from '@/i18n';

interface Props {
  available: string[];
  selected: string[];
  onToggle: (id: string) => void;
  onSelectAll: () => void;
  onClear: () => void;
}

export default function ModelPicker({ available, selected, onToggle, onSelectAll, onClear }: Props) {
  return (
    <>
      <div className="section-title" style={{ marginTop: 'var(--space-4)' }}>{t('config.selectModels')}</div>
      <div className="model-picker">
        <div className="model-picker__head">
          <button type="button" className="link-btn" onClick={onSelectAll}>{t('config.selectAll')}</button>
          <button type="button" className="link-btn" onClick={onClear}>{t('config.clear')}</button>
          <span className="muted small" style={{ marginLeft: 'auto' }}>
            {t('config.selectedModels')} {selected.length}
          </span>
        </div>
        <div className="model-picker__list">
          {available.map((m) => (
            <label key={m} className="model-row">
              <input type="checkbox" checked={selected.includes(m)} onChange={() => onToggle(m)} />
              <span className="gp-prefix">GP:</span>
              <span>{m}</span>
            </label>
          ))}
        </div>
      </div>
    </>
  );
}
