# 模块 03：供应商、预设与配置档案

审查日期：2026-11-28。在模块 02 验证完成后开始，按 `PROJECT_MODULE_REVIEW_PLAN.md` 的模块 03 范围继续；保留原有未提交改动。

## 范围

- 前端供应商新增/编辑、表单字段映射、预设和模型补全、端点测速、拖动排序、统一供应商及 Profile 对话框。
- 核心 `services/provider/` 的新增、编辑、删除、切换、原生目录协调、统一供应商同步和配置投影；对应供应商 DAO 的 ID、排序、元数据及端点处理。
- `services/profile.rs`、Profile DAO 与桌面命令的分组快照、自动保存、旧字段保留及操作串行化。
- 模型获取、图标校验与主要 IPC 入口；账号认证协议本身仍属于模块 10，通用文件原子写入仍属于模块 15。

## 发现与修改

| 严重程度 | 位置 | 触发条件、影响与依据 | 修复 |
| --- | --- | --- | --- |
| 中：新增部分提交 | `services/provider/mod.rs`，`ProviderService::add` 的首个 Claude 分支 | 先保存新行，再写客户端配置；配置损坏或不可写时返回失败，但新行已存在，用同一 ID 重试会冲突。新增测试使用真实解析器和内存 SQLite，修复前在“新行应不存在”断言失败。 | 使用现有 `keep_row_if_written`：发布前失败撤回行；已发布且存在 pending 时保留行供恢复。另加触发器注入数据库指针提交失败，验证文件已发布后保留供应商、启动恢复能够补完指针。 |
| 中：迟到结果改写连接地址 | `EndpointSpeedTest.tsx`，`runSpeedTest`、会话与保存回调 | 测速未完成即关闭弹窗、隐藏或切换供应商，旧回调仍调用表单 onChange；删除最快端点或关闭自动选择后，旧闭包仍选择它。后续保存或请求可能使用用户已放弃的地址，存在误发配置/凭据的潜在风险，本轮未观察到真实泄露。 | 按应用/供应商/可见状态隔离会话，以生命周期令牌拒绝过期结果；自动选择读取当前开关、当前剩余端点和手动选择版本；同步阻止重复测速。保存完成也不得关闭后来的弹窗。 |
| 低：重复读取与状态副作用 | `EndpointSpeedTest.tsx` 的端点读取 effect、删除处理 | `initialEndpoints` 随父表单值变化时重复读取数据库，可能重新合入已从草稿删除的端点；加载合并直接修改旧 state 内的对象，删除回调还在 state updater 内调用父组件。 | 移除无关依赖，合并时创建新对象，删除后的父回调移出 updater。新增测试确认父候选列表更新不再触发重复读取。 |
| 低：无用代码 | `useBaseUrlState.ts`、`ProviderForm.tsx`、`services/{provider/mod,profile}.rs` | Codex/Antigravity 地址已由专用 hook 管理，通用 hook 的旧分支、状态和参数无调用；`extract_credentials` 仅有自身测试，Profile 的强类型 `merge_scope_from` 已由保留未知字段的 JSON 合并替代。 | 删除上述闲置实现、Regex 导入、空回调及仅验证被删除实现的两个旧测试；保留实际凭据解析、Profile 重拍和未知字段保留的回归。 |

安全检查另覆盖模型获取的禁止重定向、响应体上限、错误脱敏、图标解码限制和保存前服务校验；这些路径本轮未确认其他新缺陷。保留仍用于历史快照兼容的非活动字段。

## 本轮验证

- 前端 58 个测试文件、1198 项通过，包含 9 项新增端点测速回归，以及表单、预设、模型补全、统一供应商、Profile、排序和配置字段回归。日志 `.git/module-03-frontend-tests.log`；已有 CodeMirror/jsdom 几何 API 提示与 act 警告未导致失败。
- Rust 核心定向测试 298 项通过，1 项原有手工性能基准忽略。范围：`services::provider::`、`services::profile::`、`provider::tests::`、供应商/Profile DAO、模型获取及 OMO。日志 `.git/module-03-core-tests.log`，复跑脚本 `.git/module-03-tests.ps1`。
- 首次添加的失败复现日志为 `.git/module-03-core-repro.log`。测速修复前的关闭、隐藏、删除端点、关闭自动选择均断言失败；同一轮供应商切换测试另因测试翻译函数不稳定超时，修正 mock 后在最终回归中通过，未将该超时当作产品缺陷证据。
- `pnpm typecheck`、5 个修改前端/测试文件的 Prettier、2 个 Rust 文件的 Rustfmt 及 `git diff --check` 通过。

Windows GNU 工具链 rustc/cargo 1.100.0，隔离目录 `target/module-03-20261128/`；未变更仓库固定工具链。未运行真实桌面、外部供应商端点、跨平台测试或安装包构建。完成本模块验证后再进入模块 04。
