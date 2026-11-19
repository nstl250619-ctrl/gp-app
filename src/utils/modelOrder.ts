// 模型展示顺序（运营口径）：
// 系列固定顺序 DeepSeek → GLM → Kimi → MiniMax → Mimo → Qwen → Doubao → 其他；
// 系列内按运营指定型号顺序，未列出的型号排在同系列已知型号之后（按字母序）。
const EXACT_ORDER = [
  'deepseek-v4.1-flash',
  'deepseek-v4-pro',
  'glm-5.3-flash',
  'glm-5.3',
  'kimi-k3.1-pro',
  'kimi-k3',
  'minimax-m3.1-flash',
  'minimax-m3',
  'mimo-v2.6-pro',
  'qwen3.8-max',
];

const EXACT_INDEX = new Map(EXACT_ORDER.map((id, i) => [id, i]));

function seriesRank(id: string): number {
  const s = id.toLowerCase();
  if (s.startsWith('deepseek')) return 10;
  if (s.startsWith('chatglm') || s.startsWith('glm')) return 20;
  if (s.startsWith('kimi') || s.startsWith('moonshot')) return 30;
  if (s.startsWith('minimax')) return 40;
  if (s.startsWith('mimo')) return 45;
  if (s.startsWith('qwen') || s.startsWith('qwq')) return 50;
  if (s.startsWith('doubao')) return 60;
  return 900;
}

export function sortModels(models: string[]): string[] {
  return [...models].sort((a, b) => {
    const ra = seriesRank(a);
    const rb = seriesRank(b);
    if (ra !== rb) return ra - rb;

    const ia = EXACT_INDEX.get(a.toLowerCase());
    const ib = EXACT_INDEX.get(b.toLowerCase());
    if (ia !== undefined || ib !== undefined) {
      if (ia === undefined) return 1;
      if (ib === undefined) return -1;
      return ia - ib;
    }
    return a.localeCompare(b, 'en');
  });
}
