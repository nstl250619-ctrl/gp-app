# Shop 端接口开发需求书 —— 绿池桌面工具对接

> 版本：v1.0（2026-10-07）
> 需求方：绿池桌面工具（GreenPool，下称"工具"）
> 交付对象：Shop 端开发（本文档可直接作为开发任务书使用）
> 状态：**工具侧已就绪（当前以 mock 运行），Shop 按本文档实现后即可联调切换**

---

## 1. 一句话背景

绿池工具是 **Shop 的桌面端延伸**：用户在 Shop 购买服务（兑换码/额度），工具负责把"核销兑换码 → 生成 API 密钥 → 一键配置到本机 AI 工具（WorkBuddy 等）→ 查询用量"这条链路傻瓜式完成。Shop 管账号与钱，工具管核销与配置。

## 2. 三方系统与职责边界（务必先读）

| 系统 | 角色 | 管什么 | 不管什么 |
|---|---|---|---|
| **绿池工具**（Tauri 桌面端） | Shop 的桌面端 + 服务核销/配置器 | 调用 Shop 接口（账号/钱包/订单）；调用 new-api 公开接口（核销/密钥/用量）；本机配置写入 | 不存储资金、不生产兑换码、不直插任何数据库 |
| **Shop**（本次开发对象） | **账号、钱包（¥）、商品、订单、发货的唯一真源** | 用户注册/登录（SMTP 邮箱验证）、钱包余额与流水、充值支付（对接支付渠道）、商品售卖、兑换码发货 | **不做兑换码核销**（核销在 new-api，见 §4） |
| **new-api**（已上线 api.greenpool.cn） | AI 中转服务 | 服务额度（quota）、用量计费、**兑换码的产出与核销**、API 密钥 | 不涉及人民币钱包；**不做任何代码修改**（工具只调其公开 API） |

## 3. 两套钱包体系（核心定义，不要合并）

| | ① 绿池钱包 = Shop 钱包 | ② 服务余额 = new-api 余额 |
|---|---|---|
| 币种 | 人民币（¥） | 服务额度（quota，站内按 ¥ 计价展示） |
| 数据源 | **Shop 数据库**，工具内"钱包充值"页直连 Shop 接口 | **new-api 数据库**，工具读 `/api/user/self` |
| 充值方式 | Shop 收银台（支付渠道回调入账） | new-api 兑换码核销（`POST /api/user/topup`） |
| 用途 | 在 Shop 购买商品（兑换码等） | 调用 AI 模型时按量扣减 |
| 关系 | **两套完全独立的体系，互不换算、互不转账** | |

工具 UI 已按此定义呈现（"钱包充值"页与"配置与用量"页分开展示两套余额）。

## 4. 兑换码生命周期（决定 Shop 不需要核销接口）

```
new-api 管理后台产出兑换码
        │  （批量生成，交给 Shop 定价上架）
        ▼
Shop 出售 → 用户支付 → 发货：兑换码绑定到用户订单（status=delivered）
        │  （用户在工具里看到自己买的码，或复制粘贴）
        ▼
绿池工具 → 调 new-api 公开接口核销：POST /api/user/topup {key: 兑换码}
        │  （Bearer = 用户在 new-api 的登录令牌；成功返回到账 quota）
        ▼
new-api 服务余额增加 → 用户调用 AI 扣减
```

**结论：Shop 只负责"出售 + 发货 + 订单记录"，不提供核销接口、不参与资金以外任何动作。**
（可选增强：工具核销成功后可回传状态给 Shop 用于售后对账，见 6.9 附注，不做不影响主流程。）

## 5. 绿池工具当前已实现的能力（Shop 开发者需要的上下文）

- 账号：注册（邮箱+验证码+密码，用户名由邮箱前缀自动生成）、登录、找回密码 —— **当前直连 new-api 公开 API，Shop 上线后切换到本合同 S1/S2**
- 密钥：登录后自动在 new-api"三步建 key"，明文只存系统钥匙串
- 兑换：已实现真核销（`POST /api/user/topup`），兑换记录读 `/api/user/topup/self`
- 用量：读 `/api/user/self`（余额/已用/次数）与 `/api/user/topup/self`
- 一键配置：探测 WorkBuddy → 备份 → 写入 → 校验 → 连接自测 → 可回滚（与 Shop 无关）
- 页面：总览（账号卡）/ 充值兑换 / 配置与用量 / 商城 / **钱包充值** / 操作记录 / 设置

## 6. 技术总约定

| 项 | 约定 |
|---|---|
| Base URL | `https://shop.greenpool.cn`（生产）；测试环境地址由 Shop 提供后写入工具配置 |
| 数据信封 | **与 new-api 完全一致**：HTTP 200 + `{"success": true, "message": "", "data": {...}}`；失败 `{"success": false, "message": "人话错误信息"}`（message 面向用户直接展示，请写中文人话，不要只给错误码） |
| 鉴权 | 除 6.1/6.2/6.3/6.4 外，一律 `Authorization: Bearer <session_token>` |
| 内容类型 | `application/json; charset=utf-8` |
| 金额 | JSON 里用**字符串两位小数**（如 `"10.00"`）；数据库内部建议以"分"整数存储 |
| 时间 | Unix 秒级时间戳（整数） |
| 限频 | 发码：同邮箱 60s/1 次、24h/10 次；登录：连续失败 5 次锁 15 分钟；下单：同用户 1s/1 次 |
| 安全 | 全站 HTTPS；密码 argon2/bcrypt；session_token ≥32 字节 CSPRNG；支付回调**验签+幂等**；日志禁止出现密码/token/验证码 |

## 7. 接口详细规格

### 7.1 POST /api/session —— 创建设备会话（S0）

- 用途：工具启动/进入钱包页时建立会话，后续所有请求携带 session_token；防滥用与设备绑定
- 请求：
```json
{ "device_id": "uuid-v4（工具首启生成并持久化）", "app_version": "0.1.0" }
```
- 响应 data：
```json
{ "session_token": "256bit 随机串", "expires_in": 86400 }
```
- 规则：同一 device_id 重复调用 → 签发新 token、旧 token 立即失效；过期后工具自动重建

### 7.2 POST /api/email/code —— 邮箱验证码（S1，走 Shop SMTP）

- 用途：注册与找回密码的发码（**这是"绿池注册=Shop 注册"的关键：验证码必须由 Shop 的 SMTP 发出**）
- 请求：
```json
{ "email": "user@qq.com", "scene": "register", "session_token": "..." }
```
`scene`: `"register" | "reset"`
- 响应 data：`null`（成功即已发送）
- 规则：6 位数字，10 分钟有效；验证失败 5 次作废；**防枚举**：reset 场景下邮箱不存在也返回 success
- 当前站点 SMTP 参考：brevo 中继 `no-reply@greenpool.cn`（Shop 可复用同一发件身份）

### 7.3 POST /api/user/register —— 注册（S2，绿池注册=Shop 注册）

- 请求：
```json
{ "email": "user@qq.com", "code": "123456", "password": "至少8位", "session_token": "..." }
```
- 行为：核验验证码 → 创建 Shop 账号 + **钱包账户（余额 0）** → 视为注册即登录
- 响应 data：
```json
{ "user_id": 10001, "username": "user", "session_token": "...", "expires_in": 86400 }
```
- 用户名规则：**请求里没有用户名**。若 Shop 需要，自动取邮箱 @ 前缀（截 20 字）；被占用则追加随机尾缀——最终 username 必须在响应中返回
- 错误：验证码错误/过期、邮箱已注册（message：`该邮箱已注册，请直接登录`）
- **重要**：Shop 注册成功后**不需要也不应该**调用 new-api。new-api 账号由绿池工具用同一 email+password 自行调 new-api 公开注册接口开通（已实现）

### 7.4 POST /api/user/login —— 登录

- 请求：`{ "email": "（或 username）", "password": "...", "session_token": "..." }`
- 响应 data：同 7.3（含新 session_token）
- 错误：账号或密码不对（人话）、被锁定（带剩余秒数）

### 7.5 GET /api/user/me —— 我的资料 + 钱包余额

- 响应 data：
```json
{
  "user_id": 10001, "email": "user@qq.com", "username": "user", "created_at": 1790000000,
  "wallet": { "balance": "12.34", "currency": "CNY" }
}
```
- 工具"钱包充值"页首屏即调此接口显示余额

### 7.6 GET /api/user/wallet/transactions —— 钱包流水

- Query：`page=1&page_size=20`
- 响应 data：
```json
{ "total": 3, "page": 1, "page_size": 20,
  "items": [
    { "id": "wt_101", "type": "topup", "amount": "10.00", "balance_after": "12.34",
      "order_id": "ord_88", "remark": "钱包充值", "created_at": 1790800000 }
  ] }
```
- `type`: `topup | purchase | refund`；`amount` 带符号（支出为负）；按 created_at 倒序

### 7.7 POST /api/wallet/topup/order —— 钱包充值下单（S4）

- 用途：工具"钱包充值"页发起充值
- 请求：`{ "amount": "10.00", "pay_channel": "alipay", "session_token": "..." }`
  `pay_channel` 枚举由 Shop 支持的渠道决定（建议至少 alipay/wxpay；通过 7.10 或约定常量暴露）
- 行为：创建待支付订单，调起 Shop 的支付渠道下单
- 响应 data：
```json
{ "order_id": "ord_88", "pay_url": "https://shop.greenpool.cn/pay/xxxx", "expires_at": 1790800900 }
```
- 工具行为：用系统浏览器打开 `pay_url`，然后轮询 7.8

### 7.8 GET /api/wallet/topup/order/{order_id} —— 查询支付状态

- 响应 data：
```json
{ "order_id": "ord_88", "status": "paid", "amount": "10.00", "paid_at": 1790800600 }
```
- `status`: `pending | paid | expired | failed`
- 支付渠道的异步回调由 Shop 自行处理：**必须验签 + 幂等**（重复回调只入账一次），入账后写流水（type=topup）
- 工具轮询间隔 3 秒、最长 5 分钟，期间用户可手动刷新

### 7.9 GET /api/user/orders —— 我的订单（含发货的兑换码）

- 用途：工具"充值兑换"页的"我买的码"与售后追溯
- Query：`page=1&page_size=20&status=delivered`
- 响应 data：
```json
{ "total": 1, "page": 1, "page_size": 20,
  "items": [
    { "order_id": "ord_90", "product_name": "¥10 服务兑换码", "amount": "10.00",
      "status": "delivered", "created_at": 1790801000,
      "codes": [ { "code_id": "rc_1", "code": "GP-XXXXXX-XXXXXX", "status": "unused" } ] }
  ] }
```
- `code` 为明文（用户要复制去兑换）；`status`: `unused | used`（used 依赖 7.9 附注的回传，未回传则恒为 unused）
- **附注（可选增强，P1）**：`PUT /api/user/codes/{code_id}/status` body `{"status":"used"}`——工具在 new-api 核销成功后回传，供 Shop 对账/售后。不做不影响主流程

### 7.10 GET /api/products —— 商品列表（P1，可选）

- 用途：工具"商城"页未来内嵌展示
- 响应 data：`{ "items": [ { "product_id": "p_1", "name": "¥10 服务兑换码", "description": "...", "price": "10.00", "stock": 999, "status": "on_sale", "pay_channels": ["alipay","wxpay"] } ] }`

### 7.11 明确不需要开发的接口：兑换核销

核销由绿池工具直调 new-api：`POST /api/user/topup`，body `{"key": "兑换码"}`，Bearer 为用户在 new-api 的令牌，成功 `data` 返回到账 quota。**Shop 不要实现任何核销端点，也不要在支付/发货之外改动用户额度。**

## 8. 关键时序（工具侧行为，供 Shop 理解调用顺序）

```
【注册】工具 → 7.1 session → 7.2 发码(Shop SMTP) → 7.3 注册(Shop 建号+钱包)
      → 工具自行：new-api /api/user/register（同邮箱密码）→ new-api 登录 → 自动建 API key
【登录】工具 → 7.1 → 7.4 → 工具自行登录 new-api → 双端就绪
【购买-兑换】用户在 Shop 网页买码付款 → Shop 发货绑订单
      → 工具 7.9 拉到 code → 用户在"充值兑换"页核销（new-api /api/user/topup）→ 服务余额到账
【钱包充值】工具 7.1 → 7.7 下单(拿 pay_url) → 系统浏览器支付 → Shop 回调入账
      → 工具 7.8 轮询到 paid → 7.5 刷新余额 → 7.6 流水出现 topup 记录
```

## 9. 安全与合规要求（验收项）

1. 密码存储 argon2id 或 bcrypt（cost ≥10）；任何接口不回传密码哈希
2. session_token / 验证码 / 支付回调签名 均不可入日志
3. 支付回调：验签 + 按 order_id 幂等 + 金额比对（回调金额 ≠ 订单金额直接拒绝并告警）
4. 下单金额校验：仅接受正数、两位小数、单笔与单日上限（上限值 Shop 自定，建议单笔 ≥1 元 ≤5000 元）
5. 发码防刷（§6 限频表）+ 图形验证码可后置（工具侧暂无人机验证）
6. 全站 HTTPS（HSTS 建议）；测试环境同样要求
7. 错误信息人话化（工具直接展示 message，不展示堆栈/内部码）

## 10. 联调验收清单（全部通过 = 交付完成）

| # | 场景 | 工具侧动作 | 预期 |
|---|---|---|---|
| 1 | 会话 | 工具启动 → 7.1 | 拿到 session_token，重复调用旧 token 失效 |
| 2 | 注册 | 总览账号卡：发码 → 注册 | 收到 Shop SMTP 邮件；注册后 me 余额 = "0.00" |
| 3 | 登录 | 同邮箱重复登录 | 成功；错误密码提示人话并计数锁定 |
| 4 | 钱包充值 | 钱包页下单 → 沙箱支付 → 轮询 | paid 后 me 余额增加；流水多一条 topup |
| 5 | 幂等 | 重放同一支付回调 | 只入账一次 |
| 6 | 订单发货 | 后台造已支付订单（含码）→ 工具拉 7.9 | 看到明文兑换码 |
| 7 | 兑换闭环 | 工具用该码调 new-api /api/user/topup | new-api 余额增加；兑换记录出现一条 |
| 8 | 异常 | 错验证码 / 过期 session / 篡改金额 | 分别返回人话错误；资金零变动 |

## 11. 工具侧现状与切换方式

- 工具当前：`services/shop.rs` 以 mock 运行（`shop_status ready=false`；兑换已直连 new-api 真核销）
- Shop 联调就绪后：工具侧新增 `HttpShopClient` 实现 7.1~7.9，替换 mock；**前端与命令面零改动**
- 工具侧涉及文件（供参考）：`services/shop.rs`、`services/account.rs`、`newapi/client.rs`、`commands/{shop,account,usage}.rs`、前端 `modules/{wallet,redeem,overview}`
- 联调环境：请提供测试 Base URL + 沙箱支付渠道 + 两个测试兑换码

## 12. 优先级

- **P0（必须，联调前提）**：7.1、7.2、7.3、7.4、7.5、7.7、7.8、7.9
- P1：7.6（流水分页完整版）、7.10（商品列表）、7.9 附注（核销状态回传）
- P2：会话刷新接口、消息通知（充值到账推送）

## 13. 联调新增需求（2026-10-07 工具侧实测后提出）

### 13.1 工具内自动登录商城（SSO，P1——二选一）

- 背景：工具已把商城页**内嵌**进客户端（iframe），且钥匙串里存有账号密码，工具侧 API 已能自动登录。
  但网页登录靠 Cookie，浏览器同源策略禁止工具把凭据"填进"跨域页面——需要 Shop 提供一个换票入口。
- **方案 A（推荐，约 10 行）**：`GET /api/sso?session_token=<登录态token>` → 校验通过后
  **Set-Cookie 写入 web 会话** 并 302 到商城首页。工具已实现探测逻辑：该端点一上线，
  内嵌页自动携带登录态（零工具改动）。
- **方案 B（已见端倪）**：实测 `POST /api/auth/login` 响应带 `access-control-allow-credentials: true`。
  若 Shop 将 `access-control-allow-origin` 精确放行工具来源（Tauri：`http://tauri.localhost` 与
  `tauri://localhost`），且登录成功响应 **Set-Cookie**，工具可直接跨域 fetch 登录，
  WebView 共享 Cookie 后内嵌页即为登录态（密码不出本地、不经 URL）。
- 安全：方案 A 的 token 建议一次性（用后作废重签）；仅 https；密码不得出现在 URL。

### 13.2 允许被工具内嵌（联调阻塞项，P0）
- 现状：工具内嵌 iframe 加载 shop.greenpool.cn。若 Shop 响应头带 `X-Frame-Options: DENY/SAMEORIGIN` 或 `CSP frame-ancestors` 限制，iframe 会被浏览器拒绝（白屏）。
- 需求：Shop 对页面响应**移除 X-Frame-Options**，或设置 `Content-Security-Policy: frame-ancestors 'self' https://*.greenpool.cn`（至少放行 Tauri 工具的来源；Tauri 内嵌来源为 app 自身，生产建议放行 `tauri://localhost` 与 `https://tauri.localhost`——Windows WebView2 场景）。
- 请 Shop 确认当前响应头并调整，工具侧已就绪（CSP frame-src 已放行）。
