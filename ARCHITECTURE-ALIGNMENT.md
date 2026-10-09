# 绿池 × Shop × new-api 架构对齐报告

- 日期：2026-10-07
- 依据：绿池工具全量代码深度检查 + 线上站点实测（api.greenpool.cn /api/status、options 表）+ new-api rc.40 源码
- 结论速览：**架构分层=设计已达成；账号/钱包主链路=过渡态（newapi 直连）今日起注册链路已真跑通；Shop 侧=契约已锁定、mock 待切换**

---

## ① 工具与 Shop 的账号/钱包/充值是否同一套？

**设计目标**：是同一套 —— Shop 是账号、钱包、充值的**唯一真源**；绿池 = Shop 桌面端 + 服务核销/傻瓜式配置延伸（买码 → 工具核对充值 → 一键配置到本机 AI 工具）。

**当前代码事实**：还没有打通，属过渡态。

| 模块 | 当前实现 | 距设计的差距 |
|---|---|---|
| 登录 | 直连 newapi `POST /api/user/login` | 未经过 Shop；Shop 上线后切换 S0 会话 + Shop 账号 |
| 钱包（余额） | 直读 newapi `/api/user/self` 的 quota | Shop 钱包为真源后由 Shop 下发/同步 |
| 充值记录 | 直读 newapi `/api/user/topup/self` | 同上 |
| 兑换码核销 | 入口 UI 已就位（兑换页），后端 mock 明确报「商城尚未开放」 | **S3 兑换端点待 Shop 上线** |
| 本机配置延伸 | **完整**：探测→计划→备份→写入→回读校验→连接自测→可回滚 | 无 |

**关于占位 tab**：当前是 **WorkBuddy（可写）+ OpenClaw / Hermes（占位）**，不是 codebuddy —— 适配器层（`adapters/` 的 `ConfigAdapter` trait：detect/plan/apply/rollback 六能力）已为多目标扩展设计好，新目标只需实现 trait + 注册 + 在 tab 数组加一项。要改名或加 codebuddy 目标，说一声即可。

## ② 注册是否调 Shop SMTP？注册=newapi 注册+建 key 是否实现？

**此前**：完全没有注册流程（只有登录）。

**本轮已实现（今天真跑通）**：`注册 → 登录 → 自动建 key` 一条龙，全走 newapi **公开 API**（newapi 端零代码修改 ✓）：

```
GET  /api/verification?email=xxx   发码（站点 SMTP：brevo 中继 no-reply@greenpool.cn）
POST /api/user/register            注册（username ≤20 / password ≥8 / email+验证码）
POST /api/user/login               登录换 PAT
POST /api/token/ + :id/key         搜同名复用或新建令牌 → 明文只进系统钥匙串
```

实测站点开关：`register_enabled ✓ / password_register_enabled ✓ / email_verification ✓（已开）/ turnstile ✗（关）/ SMTP 已配置`。

**但要注意**：当前发码走的是**中转站自己的 SMTP**，不是 Shop 的 SMTP。这与目标设计（绿池注册=Shop 注册、Shop SMTP 发码、两套账号绑定为一套）还差最后一环 —— **Shop 后端上线后**，`services/shop.rs` 已锁定 S0~S3 契约（见下），切换后前端零改动。

**给 Shop 开发的最小契约（已写进 `services/shop.rs` 代码注释）**：

| 端点 | 请求 | 职责 |
|---|---|---|
| S0 `POST /api/session` | `{device_id}` | 会话 |
| S1 `POST /api/email/code` | `{email, session_id}` | **Shop SMTP 发码** |
| S2 `POST /api/user/register` | `{email, code, password, session_id}` | Shop 建账号+钱包；返回直登 newapi 的凭据（或 Shop 代开通 newapi 账号） |
| S3 `POST /api/redeem` | `{code, session_id}` | 兑换码核销 → 加额度 |

**站点侧两个现成的坑**（你自己可控，提醒）：
- `EmailDomainRestrictionEnabled = true`：白名单目前只有默认 9 域（gmail/163/126/qq/outlook/hotmail/icloud/yahoo/foxmail），自有域名邮箱收码需先加白
- `EmailAliasRestrictionEnabled = true`：local part 含 `+` 或 `.` 的邮箱会被拒（注册页已提示）

**「旧号识别原 apikey 可选择」**：当前实现为「同名令牌（绿池自动生成）优先复用，绝不重复建号」；「列出用户全部令牌供选择」未做，列入待办。

## ③ 是否按最初设计（Shop 桌面端 + 服务插销工具，newapi 为插件模块）？

**分层架构：是，代码就是按这个组织的。**

```
commands/  命令面（ACL 收口，21 条）
services/  编排层（account/install/usage/shop）
newapi/    云服务插件模块（HTTP 契约隔离，newapi 端零改动）
adapters/  本机服务插件位（ConfigAdapter 六能力 trait；workbuddy 实现，OpenClaw/Hermes 占位）
```

**业务流程逐环节状态**（对照你给的主链路）：

| 环节 | 状态 | 说明 |
|---|---|---|
| 账号注册/登录（Shop 端） | △ 过渡 | 注册/登录已实现但直连 newapi；Shop 上线切 S0~S2 |
| 调用 newapi 账号注册 | ✅ 本轮 | 公开 API，站点实测可跑通 |
| 新号生成 apikey | ✅ | 搜同名复用优先，防重复建号 |
| 旧号识别原 apikey 可选择 | △ | 仅同名复用；列全部令牌供选=待办 |
| 完成兑换码核销充值 | ❌ | Shop mock；等 S3 |
| 查询中转站用量 | ✅ | quota/已用/调用次数 + ¥ 换算 |
| 定位本机工具与文件 | ✅ | 候选路径探测 + 环境变量覆盖 |
| 备份 → 写入 → 连接自测 → 回滚 | ✅ | 原子写+摘要校验+按记录路径回滚+互斥锁 |
| 多服务/多插件扩展位 | ✅ | trait + tab 占位 + 分组能力（站点已铺日卡/14天/月卡组） |

**后续插件接入**（支撑 Shop 售卖更多服务）：新云服务 = 新增一个 `newapi` 式模块（HTTP 契约隔离）；新本机目标 = 实现 `ConfigAdapter`。Shop 只需按 S0~S3 出后端，工具端配置化接入。

## 验证

`tsc` 0 error · `cargo check` 0 警告 · `cargo test` 10/10 · design-gate 通过 · 新命令 `account_send_code` / `account_register` 已三处注册（build.rs/capabilities/lib.rs）
