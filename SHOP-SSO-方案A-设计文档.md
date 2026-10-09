# Shop SSO 端点设计文档（方案 A）—— 工具内嵌商城自动登录

> 版本：v1.0（2026-10-07）
> 需求方：绿池桌面工具（GreenPool）
> 关联文档：《SHOP-API-REQUIREMENTS.md》§13.1 / §13.2
> 工具侧状态：**适配已全部完成，端点上线即生效，工具零改动**

---

## 1. 背景与目标

绿池工具已把商城**内嵌**进客户端（iframe，全幅显示），且钥匙串里存有用户凭据：
- 工具已能用凭据自动登录 Shop **API**（`POST /api/user/login` → `session_token`，Bearer 调接口）；
- 但**网页**登录靠 Cookie。浏览器同源策略禁止工具把凭据"填进"跨域页面，所以内嵌页打开时是未登录状态。

**目标**：提供一个约 10 行的换票端点，让工具内嵌页打开即已登录：
```
工具（有 API session_token）
   │ iframe 导航
   ▼
GET https://shop.greenpool.cn/api/sso?session_token=<token>
   │ 校验 token → 签发 web 会话
   ▼
Set-Cookie: <web 会话 Cookie>
302 Location: /            （商城首页）
   ▼
iframe 内即为已登录的商城页面
```

## 2. 接口规格

### 2.1 端点

| 项 | 值 |
|---|---|
| 方法/路径 | `GET /api/sso` |
| 鉴权 | 无 Bearer，凭 `session_token` 查询参数一次性换票 |
| Content-Type | 校验失败时 `application/json`（人话错误）；成功时无需响应体（302） |

### 2.2 请求

```
GET /api/sso?session_token=<7.3/7.4 登录返回的 session_token>
```

### 2.3 响应（成功）

```
HTTP/1.1 302 Found
Set-Cookie: <与 Web 端登录完全相同的会话 Cookie，属性见 2.5>
Location: /
```

### 2.4 响应（失败）

| 场景 | 建议 |
|---|---|
| token 无效 / 已过期 / 已用过 | `302 Location: /login`（**不要**停在 JSON 错误页——iframe 里用户看到的是商城页面，跳登录页比白屏 JSON 友好）；或 `401 {"success":false,"message":"登录态已过期"}`（工具按回落处理） |
| 参数缺失 | `302 Location: /login` 或 401，同上 |

### 2.5 Cookie 属性（关键，联调失败多半在这里）

与 Web 端正常登录下发的会话 Cookie **同名同属性**，并注意：

```
Set-Cookie: <web_session_name>=<value>;
  Path=/;
  HttpOnly;
  Secure;
  SameSite=Lax        ← 首选；若联调发现 iframe 内 Cookie 不生效，改 SameSite=None; Secure
```

- **WebView2 内嵌上下文中本请求属于第三方导航**：首选 `SameSite=Lax` 联调；
  若出现"SSO 成功 302 但页面仍是未登录"，说明该 Cookie 在 iframe 场景被 SameSite 拦截，
  改为 `SameSite=None; Secure`（必须带 Secure）。
- Cookie 生命周期 = Web 端会话生命周期（Shop 自定，建议 7 天）。

## 3. token 语义（工具已兼容两种模式，Shop 任选）

| 模式 | 说明 | 工具侧行为 |
|---|---|---|
| **一次性（推荐）** | token 换票成功即作废；重复使用返回失败 | ✅ 已兼容——工具**每次打开商城页都重新登录 API 铸新 token** 再拼 SSO 地址，绝不复用旧 token |
| 短时复用 | token 在其 TTL 内可多次换票 | ✅ 同样兼容（行为一致，只是不触发作废） |

推荐一次性：即使 URL 泄漏（服务端访问日志等），作废后无任何价值。

## 4. 安全要求

1. **仅 HTTPS**（生产已是）
2. **token 不得写入任何日志**（访问日志请脱敏 `session_token` 参数）
3. **防开放重定向**：`Location` 只允许站内路径（固定 `/` 或白名单路径），绝不回显外部 URL
4. **限频**：按 IP 与按 token 各 10 次/分钟（防爆破枚举）
5. 一次性模式：换票成功后原子作废，防止重放
6. 失败响应不得区分"token 不存在/已过期/已使用"（统一 302 /login 或统一 401 文案），防探测

## 5. 工具侧行为（已完成，供 Shop 理解，无需改动工具）

1. 打开商城页 → 哑探测 SSO 是否部署（伪 token，期待非 404；**不消耗真 token**）
2. 已部署 → 静默 API 重登铸新 token → iframe 导航 `GET /api/sso?session_token=<新token>`
3. 未部署 / 失败 → iframe 直接加载商城首页（用户在内嵌页手动登录一次，WebView 记住 Cookie）
4. 外部浏览器打开（"新窗口打开"按钮）走普通 URL（不消耗 token）

## 6. 联调验收清单

| # | 步骤 | 预期 |
|---|---|---|
| 1 | `curl -i "https://shop.greenpool.cn/api/sso?session_token=probe"` | 非 404（400/401/302 均可）——确认端点已部署 |
| 2 | 工具登录账号 → 打开商城页 | iframe 自动到达已登录的商城首页，**无需手动登录** |
| 3 | 关闭商城页再打开 | 重复步骤 2 仍自动登录（一次性 token 场景下工具重铸） |
| 4 | 用同一 token 手动 curl 两次（一次性模式） | 第二次 302 /login 或 401（已作废） |
| 5 | 伪造 token | 302 /login 或 401，不泄漏"不存在/过期"差异 |
| 6 | 退出工具账号 → 打开商城页 | iframe 回落为商城未登录态（或提示登录） |

## 7. 关联前置项（勿忘）

- **§13.2 允许被内嵌**：Shop 页面响应必须**移除 `X-Frame-Options`** 或配置
  `Content-Security-Policy: frame-ancestors 'self' https://*.greenpool.cn tauri://localhost http://tauri.localhost`，
  否则 iframe 直接白屏——SSO 做好了也看不到。建议与 SSO 一并上线。
