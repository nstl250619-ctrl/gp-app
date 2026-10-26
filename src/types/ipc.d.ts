// IPC 契约类型（骨架期最小集，后续随 openapi.yaml 生成完整版，禁手改）。
export interface CommandError {
  code: string;
  category: string;
  message: string;
  nextAction?: string;
  detail?: string;
}

export interface CommandResult<T> {
  ok: boolean;
  data?: T;
  error?: CommandError;
}

export interface AppStatus {
  name: string;
  abbr: string;
  version: string;
  loggedIn: boolean;
  username: string;
  hasApiKey: boolean;
  maskedApiKey?: string | null;
  targetDetected: boolean;
  shopReady: boolean;
  /** 账号分组（new-api user.group，如 svip/月卡组-1）；未登录或获取失败为 null */
  group?: string | null;
}

export interface InstanceCapability {
  baseUrl: string;
  reachable: boolean;
  version: string;
  serverAddress: string;
  registerEnabled: boolean;
  emailVerification: boolean;
  selfUseMode: boolean;
}

export interface AccountStatus {
  loggedIn: boolean;
  username: string;
  userId: number;
  hasApiKey: boolean;
  maskedApiKey?: string | null;
}

export interface SetupKeyResult {
  tokenName: string;
  maskedKey: string;
}

export interface KeyItem {
  tokenId: number;
  name: string;
  status: number;
  selected: boolean;
}

export interface LinkStatus {
  loggedIn: boolean;
  newapiOk: boolean;
  newapiUsername: string;
  shopReady: boolean;
  shopLinked: boolean;
  shopError?: string | null;
  hasKey: boolean;
}

export interface ResetPasswordResult {
  newPassword: string;
}

/** 单条活跃订阅（用量页订阅卡切换单元），最新在前 */
export interface SubscriptionCard {
  planId: number;
  total: number;
  used: number;
  remaining: number;
  startTime: number;
  endTime: number;
  totalUsd: number;
  usedUsd: number;
  remainingUsd: number;
}

export interface QuotaSummary {
  username: string;
  group: string;
  quota: number;
  usedQuota: number;
  remaining: number;
  requestCount: number;
  /** USD 计量（与 new-api 一致）：quota / 500000 */
  quotaUsd: number;
  usedUsd: number;
  remainingUsd: number;
  /** 订阅余额（激活订阅剩余额度，USD）；null=无激活订阅 */
  subscriptionUsd?: number | null;
  /** 订阅配额总额（USD） */
  subscriptionTotalUsd?: number | null;
  /** 订阅已用（USD） */
  subscriptionUsedUsd?: number | null;
  /** 订阅到期时间（unix 秒；null=未知）——多订阅时取最晚 */
  subscriptionEndTime?: number | null;
  /** 活跃订阅明细（最新在前）；空数组=无订阅 */
  subscriptions?: SubscriptionCard[];
}

/** install.availableModels：offline=true 表示网关不可达、当前为离线兜底清单 */
export interface AvailableModels {
  models: string[];
  offline: boolean;
}

export interface LogItem {
  id: number;
  createdAt: number;
  tokenName: string;
  modelName: string;
  quota: number;
  promptTokens: number;
  completionTokens: number;
  useTime: number;
  content: string;
  isStream: boolean;
}

/** 单日积分消耗（统计柱状图数据点） */
export interface DayStat {
  day: string;
  credits: number;
}

export interface UsageDetail {
  items: LogItem[];
  totalTokens: number;
  totalRequests: number;
  totalQuota: number;
  totalQuotaUsd: number;
  total: number;
  truncated: boolean;
  /** 失败次数（type=5 错误日志，同时间范围） */
  errorCount: number;
  /** 按天积分聚合（全量口径，不受明细 100 条限制） */
  daily: DayStat[];
}

export interface RedemptionRecord {
  createdAt: number;
  content: string;
  amountUsd: number | null;
  /** 兑换码有效期（unix 秒；null=未知，0=长期有效） */
  expiresAt?: number | null;
}

export interface TopUpRecord {
  money: string;
  amount: number;
  createTime: number;
  tradeNo: string;
  status: string;
}

export interface ShopStatus {
  ready: boolean;
  baseUrl: string;
  ssoReady: boolean;
  message: string;
}

export interface ProductItem {
  id: number;
  title: string;
  price: number;
  comparePrice?: number | null;
  description: string;
  fulfillType: string;
}

export interface ProductDetail {
  id: number;
  title: string;
  price: number;
  description: string;
  skus: { id: number; name: string; price: number }[];
}

export interface BuyResult {
  orderNo: string;
  amount: number;
  status: string;
  deliveryText: string;
  /** 一键购买：网关是否已自动核销兑换码 */
  redeemed: boolean;
  /** 自动激活的订阅套餐 id（2=尝鲜 3=周卡 4=月卡 5=日卡） */
  planId?: number | null;
  /** 自动核销到账额度（USD） */
  grantedUsd: number;
}

export interface OrderCode {
  codeId: string;
  code: string;
  status: string;
}

export interface ShopOrderItem {
  orderId: string;
  productName: string;
  amount: string;
  status: string;
  createdAt: number;
  codes: OrderCode[];
}

export interface ShopUser {
  userId: number;
  email: string;
  username: string;
}

export interface WalletSummary {
  user: ShopUser;
  balance: string;
  currency: string;
  rate: string;
  balanceYuan: string;
}

export interface TopupOrder {
  orderId: string;
  /** 支付中转页（系统浏览器打开扫码/跳转） */
  payUrl: string;
  /** 真二维码内容（weixin:// / alipays:// 等），前端直接渲染 */
  codeUrl?: string;
  expiresAt: number;
}

export interface TopupStatus {
  orderId: string;
  /** pending / paid / expired */
  status: string;
  /** ¥ 实付 */
  amount: string;
  paidAt?: number | null;
}

export interface RedeemResult {
  quotaGranted: number;
  grantedUsd: number;
  subscriptionActive: boolean;
  /** 激活的订阅套餐 id（2=尝鲜 3=周卡 4=月卡 5=日卡） */
  planId?: number | null;
}

export interface DetectResult {
  target: string;
  displayName: string;
  path?: string;
  detected: boolean;
  hasManagedEntry: boolean;
}

export interface DiffRow {
  field: string;
  before: string;
  after: string;
}

export interface InstallPlan {
  target: string;
  path: string;
  added: string[];
  overwritten: number;
  preserved: number;
  requiresRestart: boolean;
  warnings: string[];
  diff: DiffRow[];
  relatedLite?: string | null;
  relatedReasoning?: string | null;
  baseUrlBefore?: string | null;
  baseUrlAfter: string;
  keyBefore?: string | null;
  keyAfter: string;
}

export interface ApplyResult {
  status: string;
  path: string;
  backupPath?: string;
  verifyOk: boolean;
  requiresRestart: boolean;
  message: string;
}

export interface InstallRecord {
  id: string;
  target: string;
  targetPath?: string;
  createdAt: string;
  status: string;
  backupPath?: string;
}

export interface CredentialStatus {
  available: boolean;
  hasApiKey: boolean;
}

// ---- 隧道子系统（GPHub 融合）----

export type TunnelPhase =
  | 'stopped'
  | 'connecting'
  | 'connected'
  | 'degraded'
  | 'reconnecting'
  | 'error';

export type ApiState = 'ok' | 'fail' | 'unchecked';

export interface Forward {
  id: string;
  name: string;
  localPort: number;
  targetPort: number;
  enabled: boolean;
  probePath?: string | null;
  probeSuccessStatus: number[];
}

export interface HealthPoint {
  endpoint: string;
  listenOk: boolean;
  apiState: ApiState;
  lastError?: string | null;
}

export interface TunnelStatus {
  phase: TunnelPhase;
  message: string;
  connectedAt?: number | null;
  lastConnectMs?: number | null;
  hasKey: boolean;
  reconnectCount: number;
  sshAlive: boolean;
  health: HealthPoint[];
  forwards: Forward[];
}

export interface ProbeResult {
  ok: boolean;
  listenMs?: number | null;
  apiMs?: number | null;
  error?: string | null;
}

export interface ImportKeyResult {
  ok: boolean;
  keyFingerprint?: string | null;
}
