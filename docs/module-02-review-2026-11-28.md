# 模块 02：前端 API、Query 与共享状态

审查日期：2026-11-28。按照 `PROJECT_MODULE_REVIEW_PLAN.md` 顺序执行，在模块 01 已有工作树上继续。本报告仅记录本轮发现、改动和验证；不将此前的大量未提交改动算作本轮修复。

## 范围

- `src/lib/api/` 的命令注册对应关系、主要读写参数及错误契约；对照桌面命令签名。
- `src/lib/query/` 的应用隔离、写后失效、原生刷新去重、Profile 事件、会话流与图片缓存、额度查询身份及最后成功值。
- `windowActivity.ts`、`localCache.ts`、`tauriEventCleanup.ts`、`useTauriEvent.ts`、`useLatestRef.ts`、`useLastValidValue.ts`、`useDelayedFlag.ts` 及共享缓存桥接 hook。
- `appCapabilities.ts` 与相关共享类型；业务规则仍随后续对应模块审查。

## 发现与修改

| 严重程度 | 位置 | 触发条件、影响与依据 | 修复 |
| --- | --- | --- | --- |
| 中：状态错误 | `src/lib/query/failover.ts`，`useSetAutoFailoverEnabled` | 首次查询未返回时乐观启用开关，写入失败；`removeQueries` 删除正在被其他组件订阅的查询，后续失效无法刷新，独立读取组件一直显示已启用。将读写 hook 放在独立组件中后，修复前回归稳定失败（实际 true，预期 false）。 | 冷缓存失败时使用 `resetQueries` 重置并读取实际状态，保留订阅；有旧值时仍按原逻辑回滚、失效。迟到的旧查询不得覆盖实际状态。 |
| 中：重复 IPC | `src/lib/windowActivity.ts`，`refreshVisibility` | 原实现每次窗口 resize/focus 通知并发查询可见和最小化状态。30 次通知叠加首次读取共 31 组请求，已有 revision 仅丢弃结果而不限制请求。 | 同时至多一组读取；期间通知合并为一次后续读取，过期快照不发布；保留清理与后台轮询语义。测试中由 31 组降至 2 组（62 次 IPC 降至 4 次），未测量实际桌面性能收益。 |
| 低：闲置代码与冗余分支 | `query/{failover,proxy,prompts,providerCache,mutations}.ts`、`api/{settings,providers}.ts` | 通过 TypeScript 标识符扫描和全仓引用核查，发现无生产和测试调用的旧 API/hook；新增供应商已在解构中剔除 providerKey，后续 `any` 删除无作用。 | 删除 `useCircuitBreakerConfig`、`useCircuitBreakerStats`、`useDirectProviderId`、`promptAppOf`、`usePromptListQuery`、`enabledPromptOf`、`getClaudeCodeConfigPath`、`getAppConfigPath`、`getAutoLaunchStatus`、`ensureCodexOfficialProvider` 共 10 个函数，以及相应闲置类型、哨兵、空 `else` 和重复删除语句。 |

安全核查涉及外链协议限制、缓存内容及查询身份隔离；本轮未在这些路径中确认新的凭据泄露或权限绕过。历史数据兼容、仍有调用的后端命令和测试接口不因名称旧而删除。

## 本轮验证

- 18 个前端测试文件、163 项测试全部通过；日志 `.git/module-02-tests.log`。包含新增的冷缓存失败回归、窗口通知合并，以及应用切换、初次查询竞态、原生刷新、会话流/图片、事件清理、API 和最后成功值测试。
- 新增回归修复前分别失败；日志 `.git/module-02-repro.log`、`.git/module-02-cache-repro.log`。
- `pnpm typecheck`、10 个本轮修改文件的 Prettier 检查和 `git diff --check` 通过。
- API 目录静态检查覆盖 278 处字面量 invoke、273 个不同命令，均存在桌面注册入口；结果 `.git/module-02-ipc-scan.json`。这不代表所有返回类型都经过真实 IPC 运行验证。

平台为 Windows，测试使用 jsdom 和模拟 IPC；本模块未改 Rust，未运行 Rust 测试或真实 WebView。只在本模块验证完成后进入模块 03。
