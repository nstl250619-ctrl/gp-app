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
  /** 订阅到期时间（unix 秒；null=未知） */
  subscriptionEndTime?: number | null;
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

export interface RedeemResult {
  quotaGranted: number;
  grantedUsd: number;
  subscriptionActive: boolean;
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
