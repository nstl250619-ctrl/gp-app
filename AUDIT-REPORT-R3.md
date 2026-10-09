# 批次深度审查报告（第 3 轮）

- 日期：2026-10-07
- 审查范围：Shop 接入（ShopClient/device_session/client_login/wallet/bind/sso_url）、SSO 方案 A 适配、邮箱优先 + 邮箱落钥匙串、自动补关联、设备会话失效自愈、登出清 WebView Cookie、老账号绑定商城（bind flow）、页面保活（keep-alive）
- 验证：`tsc` 0 · design-gate passed · `cargo` 0 警告 · 测试 10/10 · 窗口运行中

---

## 一、高危（1 项，已修复）

### H1. 绑定时自定义 Shop 密码未持久化 → 会话过期后静默重登必然失败【业务逻辑漏洞】
- **位置**：`services/shop.rs::bind()` × `wallet()` × `account.rs::link_status()` × `sso_url()`
- **缺陷链**：用户绑定商城时如果 Shop 密码与 new-api 不同，填了自定义密码 → bind 用它登录成功 → **但没存钥匙串** → 会话过期后所有静默重登（钱包页 link_status、sso_url）退回用 new-api 密码 → Shop 拒绝 → 永久"未关联"
- **修复**：
  1. 新增 `CREDENTIAL_SHOP_PASSWORD` 钥匙串条目
  2. `bind()` 成功时持久化自定义 Shop 密码
  3. 新增 `stored_shop_password()` 统一优先级：Shop 密码 → 退回 new-api 密码
  4. `wallet()` 重登、`sso_url()` 重铸、`link_status()` 自动补关联**全部改用 `stored_shop_password()`**
  5. `logout()` 清掉 `shop_password`

## 二、中危（2 项，已修复）

| # | 问题 | 修复 |
|---|---|---|
| M1 | `ensure_session()` 每次操作都写钥匙串补齐邮箱——桌面低频但累积写入，且 Windows Credential Manager 写入有 IPC 开销 | 只在 email 未缓存时写一次（`credential::get` 判空后跳过） |
| M2 | 操作记录页（RecordsPage）无刷新按钮——保活后数据可能过期（一键配置后切回操作记录仍显示旧列表） | 加刷新按钮（与总览同款） |

## 三、低危（1 项，已修复）

| # | 问题 | 修复 |
|---|---|---|
| L1 | `sso_url()` 用 `CREDENTIAL_ACCOUNT_USERNAME`（用户名）作 Shop 登录标识——Shop 是邮箱体系，用户名查无此人 | 改为 email 优先 + username 兜底 |

## 四、核销的疑点（审查过、判定安全/可接受）

| 疑点 | 结论 |
|---|---|
| ShopPage 首次访问未登录时 SSO 落到 login 页；用户后来登录后切回商城页不重新铸 SSO | 保活模式下 ShopPage 只 mount 一次 → effect 只跑一次 → 已登录后切回不会重新触发 SSO。**边缘 case**：用户先访商城（未登录）→ 回总览登录 → 再切商城 → iframe 仍是 login 页。**缓解**：用户点商城页内「新窗口打开」或在商城 iframe 内手动登录一次（Cookie 持久）。不阻断主流程 |
| 保活后各页数据不自动刷新（如钱包余额/操作记录/用量） | 已有手动刷新按钮覆盖所有页面。保活设计意图就是"不自动刷新，靠手动刷新"——切页快是首要目标 |
| `is_session_error` 匹配 `"会话"` 偏宽 | Shop 当前 message 体系只有"会话已过期"含该词；业务错误不含。实际影响面≈0 |
| visited-set 首次导航有 1 帧空渲染 | useEffect 在渲染后更新 visited → 首次切页有极短空帧再 mount → 用户感知为"首次切页略慢、之后秒切"。符合预期 |
| `bind()` 重试验证码是否被消费 | Shop 先验 session 后验 code（v1.1 设计），session 失效失败不消费 code → 重试安全。若 Shop 改变顺序则有风险，当前安全 |

## 五、遗留未修（与第 2 轮一致，低优先级）

1. OpenClaw/Hermes 适配器占位（P1）
2. 安装记录 SQLite 化（P2）
3. ipc.d.ts 由 openapi.yaml 生成（P2）
4. seeds_for 模型能力一刀切 true（P2）
5. 前后端默认 key 选择口径不一致（L4，不影响功能）

## 六、结论

本批次（Shop 接入 + SSO + 绑定 + 保活）引入了 **1 个高危业务漏洞**（自定义 Shop 密码未持久化 → 会话过期后无法静默重登）——这正是"单功能测试通过、交叉审查才能暴露"的典型：绑定功能本身正常（首次能登录），但跨"绑定 × 会话过期 × 静默重登"三个功能组合才炸。已修复并验证。

其余 3 项中低危均为体验/效率优化，不影响核心功能链路。全部修复完成。
