# 绿池（GreenPool）深度代码审计报告

- 审计日期：2026-10-07
- 审计范围：`src-tauri/src`（Rust 后端全部 18 个模块）+ `src`（前端全部）+ `tauri.conf.json` + `build.rs` + `capabilities` + 设计门禁
- 交叉验证基准：new-api `v1.0.0-rc.40` 源码（`_ref_newapi`，与线上版本一致）
- 修复状态：**15 项全部修复，10/10 单测通过，tsc 0 error，design-gate 通过，dev 运行验证通过**

---

## 一、高危（5 项，全部修复）

### H1. `account_setup_key` 在真实站点必然失败【业务断链】
- **位置**：`services/account.rs` + `newapi/client.rs::create_token`
- **根因**：new-api 的 `POST /api/token/`（`controller/token.go:355-358`）成功时用 `c.JSON` 直接返回 `{"success":true,"message":""}` —— **根本没有 data 键、不返回新令牌 id**。而客户端的 `parse_envelope` 要求 `env.data` 必在 → 必然 Err("中转站响应缺少数据")。
- **影响**：核心主链路「登录 → 生成密钥 → 一键写入」在第二步断掉，整个产品的核心功能不可用。
- **修复**：
  1. 新增 `post_expect_ok`（只验 success 的写接口专用）
  2. 新增 `search_tokens(keyword)`（`GET /api/token/search`，分页信封 `{items:[{id,name,status}]}`）
  3. `setup_key` 改为四步：**搜同名启用令牌 → 命中直接取密钥（复用）→ 未命中才创建 → 再搜定位 id → 取密钥**
  - 顺带修复 M6：重复点「生成密钥」不再无限建号（new-api 有令牌数量上限，超限报"已达到最大令牌数量限制"）

### H2. `rollback` 数据错位【写错文件】
- **位置**：`services/install.rs::rollback` + `adapters/workbuddy.rs::rollback`
- **根因**：还原时用 `existing_path()` **重新探测**配置路径，而非备份的来源路径。三种场景出错：① 用户装了两个版本 WorkBuddy，备份来自 A、还原写进 B；② A 被移走只剩 B → 同样错位；③ 全部移走 → `TargetNotFound` 无法还原（但备份文件明明还在）。
- **修复**：`InstallRecord` 新增 `target_path`（serde default 兼容旧记录）；`ApplyResult` 新增 `path`；rollback **按记录的原路径还原**，旧记录（无 target_path）回退探测。trait 签名改为 `rollback(backup, target)`。

### H3. `apply` 违反自家铁律，静默清空用户配置【数据丢失】
- **位置**：`adapters/workbuddy.rs::apply`（原 103 行）
- **根因**：`json_merge.rs` 注释白纸黑字写着「availableModels 尊重现状，写入方**绝不主动收敛**（§11.3 血的教训）」，但 apply 却 `json!({"models": merged, "availableModels": []})` —— **直接写死空数组**。用户如果自定义了下拉显示范围（非空 availableModels），一键写入会静默清掉。同时整文件重写丢弃 `models`/`availableModels` 之外的**所有顶层字段**（WorkBuddy 未来新增字段即丢数据）。
- **修复**：新增 `rebuild_outer(raw, merged_models)` 做 **key 级重建**——以原文件对象为底，只覆盖 `models`；其他顶层字段原样保留；`availableModels` 原样保留（缺失才补 `[]`）。回读校验同步改为「托管条目存在 + availableModels 与写前一致」。新增 2 个单测锁定行为（未知字段保留 / 用户下拉范围保留）。

### H4. `app_open_external` 无域名白名单【安全】
- **位置**：`commands/app.rs`
- **根因**：任意 URL 字符串直接进 `opener().open_url()`，`file://`、`smb://`、恶意域名都能用系统默认程序打开（链接来源若被注入，等于给前端一个"打开任意东西"的原语）。
- **修复**：纯标准库解析（去 scheme/userinfo/端口/path），**只放行 `https://` + `greenpool.cn` 或其子域**，其余返回"仅允许打开绿池官方站点的链接"。

### H5. 记录文件非原子写 + 无并发锁 + 损坏静默清空【数据丢失】
- **位置**：`services/install.rs`
- **根因**：三连：① `save_records` 用 `std::fs::write`，崩溃时半写坏；② `load_records` 用 `unwrap_or_default()`，文件一坏**静默清空全部历史**（所有备份引用全丢，且用户无感知）；③ apply/rollback 可并发触发，读-改-写竞态互相覆盖。
- **修复**：① `save_records` 改 `fs_atomic::atomic_write`（临时文件+sync+persist）；② 损坏时**明确报错**（提示可删除重试），不再静默吞掉；③ 模块级 `static INSTALL_LOCK: Mutex<()>`，apply 的同步写入段与 rollback 全程持锁，**锁不跨 await**（async 命令要求 `Send`）。

## 二、中危（7 项，全部修复）

| # | 问题 | 修复 |
|---|---|---|
| M1 | `create_token` group 硬编码 `"default"` —— 用户分组非 default 时令牌权限错位 | 改传 `""`（new-api 约定：空分组=跟随用户分组） |
| M2 | login 只认 Gen B 的 `access_token`，旧版（Gen A）返回 `accessToken` 会解析为空 → 报"登录响应缺少令牌" | `#[serde(alias = "accessToken")]` |
| M3 | 前端登录成功后拉额度失败，catch 显示在登录表单里 → 用户以为登录失败 | quota 拉取拆独立 try/catch，失败置 null 不报错 |
| M4 | rollback 用 `fs::copy` 直写目标（中途失败=半写坏文件） | 改 `atomic_write`（备份内容原子写回）+ digest 校验 |
| M5 | IPC 错误 `category` 全部硬编码 `"server"`，前端无法按类别分流（网络问题提示重试/用户问题提示改输入） | Network→`network`，KeyMissing/TargetNotFound→`user`，其余 `server` |
| M6 | 重复点击「生成密钥」无限创建同名令牌 | 并入 H1 的 search-or-create |
| M7 | `app_status` 只看第一个适配器（`first()`） | 改 `.any()`（全部适配器任一探测到即 true，为 P1 多适配器铺路） |

## 三、低危（3 项，全部修复）

| # | 问题 | 修复 |
|---|---|---|
| L1 | 历史记录 `rolled_back` 后回滚按钮仍可点（二次点击才报错） | `status === 'rolled_back'` 时 disabled |
| L2 | `account_login` 空用户名/密码直接发请求（浪费一次 401） | 后端先校验非空（trim） |
| L3 | `ipc.d.ts` 缺 `targetPath`/`path` 字段 | 补齐 |

## 四、审计过但判定为「按设计接受」的项（附理由）

| 项 | 理由 |
|---|---|
| API key 明文写入 models.json | WorkBuddy 协议要求；文件已 icacls/0600 收紧（§13.4 已知风险） |
| 密码经 Tauri IPC 明文传递 | 本地 IPC，进程内可见性无增益（§13 已知） |
| `app_track_event` 只记本地日志 | 合规设计（§13.3），无 IP/原文 |
| `seeds_for` 全模型能力位一刀切 true | WorkBuddy 仅用于列表展示，当前 13 个模型均支持三能力；后续接模型能力表再细化 |
| 历史记录无限增长 | 单用户桌面场景，年量级千条内，无性能问题 |
| `devCsp: null` | 仅 dev 模式；生产 CSP 已锁 `connect-src` 到 api/shop.greenpool.cn |
| 验证：PAT 过 UserAuth | **已实测源码确认**：`authenticateDashboardRequest` 支持 Bearer PAT（`middleware/auth.go:47-68`），`/api/user/self`、`/api/token/*` 均可用 |
| `toSnakeCommand` 点号命令转换 | 逐条核对 18 个命令名，全部正确映射 |

## 五、修复验证

| 验收项 | 结果 |
|---|---|
| `cargo check` | 0 error / 0 warning ✅ |
| `cargo test --lib` | **10 passed / 0 failed**（含 2 个新增数据保全测试）✅ |
| `tsc --noEmit` | 0 error ✅ |
| `design-gate.ps1` | passed ✅ |
| dev 运行 | 窗口进程存活、vite 模块全部 200 ✅ |

## 六、遗留建议（非缺陷，下阶段优化）

1. **真机回归**：用一个真实账号走「登录 → 生成密钥（应复用不重建）→ 一键写入（含自定义 availableModels 的文件应原样保留）→ 还原」全链路
2. **令牌复用的边界**：若用户在站点手动删除了「绿池自动生成」再点生成，会新建——符合预期；若同名令牌被禁用（status≠1），当前逻辑也会新建，可考虑提示
3. **模型能力表**：接入 `GET /api/user/models` 后（todo #6 收尾），`seeds_for` 应改为按模型真实能力填充 `supports_*`
4. `error.rs` 的 `Other(String)` 承载了过多语义（登录失败/建号失败/损坏提示），引入 §14 完整错误码表时可拆分
