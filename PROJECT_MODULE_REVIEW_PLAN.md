# 项目模块列表与逐模块审查范围

核对日期：2026-11-28。本文维护模块编号、代码入口和职责边界，作为后续逐模块分析的索引。当前架构见 [architecture.md](docs/architecture.md)，可复跑测试命令见 [testing.md](docs/testing.md)。当前发行版应用入口收敛为五个客户端（Claude Code、Codex、OpenCode、DeepSeek Harness、Antigravity），AppType 仅保留这五个变体，历史标识由 is_retired 识别，用于跳过旧持久化记录。

本文不维护易过期的审查进度表，也不以历史测试结果宣称当前工作树已通过。每次审查应针对当时的代码和测试，单独说明发现、修复及实际验证范围。

## 一、项目结构与执行方式

项目由 React/TypeScript 前端、Tauri 桌面适配层、Rust 核心库及独立 CLI 组成：

| 层次         | 主要位置                                                         | 职责                                  |
| ------------ | ---------------------------------------------------------------- | ------------------------------------- |
| 主窗口前端   | `src/`                                                           | 页面、交互、状态、缓存、IPC 调用      |
| 托盘面板前端 | `src/tray-panel/`                                                | 独立托盘窗口与快捷操作                |
| 桌面适配层   | `src-tauri/src/`                                                 | Tauri 命令、窗口、系统集成及 GUI 服务 |
| 共用后端核心 | `crates/cc-switch-core/src/`                                     | 数据库、配置、业务服务、代理、会话等  |
| 独立命令行   | `crates/cc-switch-cli/src/`                                      | CLI 参数和命令执行                    |
| 验证与交付   | `tests/`、`src-tauri/tests/`、源码内测试、`scripts/`、`.github/` | 回归验证、构建、打包、发布            |

审查以功能职责划分，不把一个目录简单视为一个模块。每次只推进一个模块，顺序为：确认范围 → 深入审查 → 说明发现 → 修改代码 → 验证。每项发现包含文件与函数/行号、触发条件、具体表现、潜在影响、严重程度及验证依据。

每个模块均检查代码质量、逻辑正确性、边界条件、错误处理、性能与可维护性；涉及数据写入的模块额外检查并发、事务、恢复与兼容性。业务模块内对应的测试随模块一起审查，并按当前授权执行验证；明确区分类型检查、编译检查、单元测试和完整打包。

下列路径为入口和主要范围，包含其同职责辅助文件、类型及测试；共享文件按函数职责分配。跨模块调用可以沿调用链核查，但不把一次局部核查算作整个被依赖模块已审完。

## 二、模块总表

| 顺序 | 模块                             | 包含的核心内容                                     |
| ---- | -------------------------------- | -------------------------------------------------- |
| 01   | 启动、生命周期与导航外壳         | 启动初始化、恢复、退出、更新重启、主窗口导航       |
| 02   | 前端 API、Query 与共享状态       | IPC 契约、查询缓存、变更失效、事件订阅、公共状态   |
| 03   | 供应商、预设与配置档案           | 供应商管理、表单、默认项、预设、模型信息、Profiles |
| 04   | 路由、代理与故障切换             | 直连/路由/聚合、请求转发、协议转换、熔断与恢复     |
| 05   | MCP 管理与连通性                 | 服务器增删改、应用启用、原生同步、连接测试         |
| 06   | Skills 管理与存储                | 发现安装、启用卸载、SSOT、更新、仓库与存储迁移     |
| 07   | 提示词、工作区与记忆             | 提示词管理、文件编辑、工作区与应用记忆             |
| 08   | 会话管理与阅读器                 | 会话发现解析、搜索、阅读、图片、历史与压缩         |
| 09   | 用量、配额与定价                 | 请求计量、会话用量、余额配额、成本和汇率           |
| 10   | 授权与账号管理                   | OAuth、登录、凭据刷新、账号切换与安全存储          |
| 11   | 设置、导入导出与云同步           | 设置持久化、备份恢复、S3/WebDAV、同步冲突          |
| 12   | CLI、工具发现与进程生命周期      | 独立 CLI、工具安装更新卸载、终端、进程管理         |
| 13   | 托盘、深链接与平台集成           | 托盘菜单/面板、URL 导入、窗口、轻量模式、自启      |
| 14   | 数据库、Schema 与持久化基础      | 连接、事务、迁移、DAO 契约、备份完整性             |
| 15   | 客户端原生配置与文件写入基础     | JSON/TOML/env、live 补丁、冲突检测、原子写入与恢复 |
| 16   | 通用界面、国际化、诊断与构建测试 | 公共 UI、错误展示、日志、翻译、测试设施、构建发布  |

说明：保留原审查记录的 01–13 顺序；将原先未单列的数据库和原生配置基础层列为 14、15，原通用 UI/构建项调整为 16。本表作为后续统一编号依据。

## 三、逐模块范围

### 模块 01：启动、生命周期与导航外壳

**包含什么：** 从进程启动到业务页面可用的编排；后端状态注册；数据库预检与旧配置迁移入口；首次启动导入；启动失败恢复；静默启动与显式打开的优先级；关闭、退出清理、更新安装与重启；侧栏、设置页、托盘导航以及未保存内容保护。

**主要代码：**

- `src/main.tsx`：前端启动、初始化错误处理与根组件挂载。
- `src/App.tsx`、`src/components/shell/`：主界面外壳、页面切换、应用选择和业务入口。
- `src/lib/navigation.ts`、`src/lib/unsavedChanges.ts`、`src/hooks/useTrayNavigation.ts`、`src/hooks/useStartupFailures.ts`：导航与启动反馈。
- `src/contexts/UpdateContext.tsx`、`src/lib/updater.ts`、`src/components/DatabaseUpgrade.tsx`：应用更新和启动恢复界面。
- `src-tauri/src/main.rs`、`lib.rs`、`startup.rs`、`lifecycle.rs`、`init_status.rs`：桌面启动、就绪状态与退出协调；后两者负责启动编排与生命周期协调。
- `src-tauri/src/commands/settings.rs` 中更新/重启命令，`crates/cc-switch-core/src/startup.rs` 中相关启动逻辑。
- 核心 `startup::open_database` 负责装配数据库变更观察者和本地定价同步；`store.rs` 负责共享状态及弱引用，`runtime_hooks.rs` 负责组合宿主能力。GUI 与 CLI 都使用此装配入口。

**重点：** 初始化顺序、线程约束、状态未就绪调用、异步竞态、迁移失败重试、启动阻塞、失败后的退出恢复、导航与表单所属应用一致性。

**边界：** 本模块追踪启动所需的 Skills 迁移与代理恢复，但 Skills 全量业务归 06，代理全量业务归 04；数据库迁移基础设施归 14。

### 模块 02：前端 API、Query 与共享状态

**包含什么：** TypeScript 与 Tauri 命令之间的数据契约；Query Key、缓存隔离、查询启停、重试和轮询；mutation 成功后的缓存更新；事件订阅与清理；本地缓存及共享 hook 的生命周期。

**主要代码：**

- `src/lib/api/`：命令名、参数、返回值和错误契约。
- `src/lib/query/`：`queryClient.ts`、`queries.ts`、`mutations.ts` 及领域查询文件。
- `providerActions.ts` 管理显式原生同步、删除及操作后刷新；`providerCache.ts` 管理缓存失效规则；`proxyKeys.ts`、`usageKeys.ts` 提供独立查询键。
- `src/config/appCapabilities.ts` 是无 React 依赖的应用身份与能力入口；`appConfig.tsx` 保留展示配置与兼容导出。
- `src/lib/localCache.ts`、`src/lib/windowActivity.ts`。
- `src/hooks/useTauriEvent.ts`、`useLastValidValue.ts`、`useLatestRef.ts`、`useDelayedFlag.ts` 等通用 hook。
- `src/types.ts`、`src/types/`、`src/lib/api/types.ts`，以及后端对应命令签名。

**重点：** 切应用时串缓存、旧请求覆盖新数据、卸载后更新、重复监听、过度轮询、错误被吞、类型断言掩盖运行时数据不符。

**边界：** 这里检查通信与状态机制；供应商、MCP、会话等具体业务规则在对应模块审查。

### 模块 03：供应商、预设与配置档案

**包含什么：** 供应商新增、编辑、删除、复制、排序、切换和默认项；通用供应商；预设与模型信息；Profiles；不同应用表单的字段映射；OMO 配置入口。

**主要代码：**

- `src/components/providers/`、`src/components/universal/`、`src/components/profiles/`。
- `src/config/*ProviderPresets.ts`、`codexTemplates.ts`、`preset*.ts`、`appConfig.tsx` 及相关模型目录配置。
- `src/hooks/useProviderActions.ts`、`useModelMetadataFill.ts`；`src/lib/schemas/provider.ts`、`modelMetadata.ts`。
- `src-tauri/src/commands/provider.rs`、`profile.rs`、`omo.rs`、`model_fetch.rs`、`custom_icon.rs`；`src-tauri/src/services/profile.rs` 仅保留核心服务的兼容导出。
- `crates/cc-switch-core/src/provider.rs`、`services/provider/`、`services/profile.rs`、`services/omo.rs`、`services/model_fetch.rs`。
- `services/provider/catalog.rs` 区分 `list` 只读快照与 `refresh` 原生协调；`desktop_import.rs` 承担 Claude 到 Desktop 的导入转换。
- `src/lib/providerConfig.ts` 承担纯配置识别与转换；核心 `provider_identity.rs`、`icon.rs` 保存共享身份判断和图标规则。
- 对应 `database/dao/providers*.rs`、`universal_providers.rs`、`profiles.rs`、`custom_icons.rs` 的业务读写规则。

**重点：** ID 冲突、错误应用写入、表单字段丢失、预设兼容、切换回滚、并发编辑、排序一致性、重复初始化。

**边界：** 请求转发与模式控制归 04；账号凭据管理归 10；通用原生文件读写归 15。

### 模块 04：路由、代理与故障切换

**包含什么：** 直连、路由、Stack 聚合；代理接管与解除；HTTP 请求与流式响应；协议转换、上游选择、连接复用；故障切换、熔断、重试、错误映射与代理访问控制。

**主要代码：**

- `src/components/proxy/`、`src/components/providers/mode/`；`src/hooks/useModeActions.ts`、`useProxyStatus.ts`、`useGlobalProxy.ts`。
- `src-tauri/src/commands/proxy.rs`、`failover.rs`、`global_proxy.rs`。
- `crates/cc-switch-core/src/mode/`、`provider_routing.rs`、`services/proxy.rs`。
- `crates/cc-switch-core/src/proxy/` 中 server、forwarder、handlers、response_processor、provider_router、circuit_breaker、failover_switch、HTTP 客户端及协议适配器。
- 对应 `database/dao/proxy.rs`、`failover.rs`。
- `coordination.rs` / `AppState.operations` 管理共享配置操作锁；`services/failover.rs` 实现核心故障转移执行器，`runtime_hooks.rs` 定义请求与接口。
- `model_names.rs` 承担代理、模式和配置投影共用的模型命名语义；`proxy_config.rs` 承担共享配置与状态类型。

**重点：** 流式取消、背压、超时、请求体重放、重复重试、副作用请求、代理自环、并发切换、客户端恢复失败、连接与内存泄漏。

**边界：** 认证刷新协议归 10；token/成本统计归 09；live 文件补丁基础归 15。

### 模块 05：MCP 管理与连通性

**包含什么：** MCP 服务定义、预设、校验、导入导出、按应用启用及同步；stdio/HTTP/SSE 连接测试与错误反馈。

**主要代码：**

- `src/components/mcp/`、`src/hooks/useMcp.ts`、`useMcpConnectivity.ts`、`src/config/mcpPresets.ts`、`src/lib/api/mcp.ts`。
- `src-tauri/src/commands/mcp.rs`、`src-tauri/src/services/mcp_connectivity.rs` 及其测试目录。
- `crates/cc-switch-core/src/services/mcp.rs`、`services/mcp/commit.rs`、`mcp/`、`claude_mcp.rs`、`gemini_mcp.rs`。
- `McpWritePlan` 集中增改、删除、启用操作的提交顺序；客户端适配器仍负责校验、原生锁及补偿恢复。
- `crates/cc-switch-core/src/database/dao/mcp.rs`。

**重点：** 服务名冲突、传输类型转换、环境变量/命令参数、非法配置、部分应用写入失败、连接进程回收、重试与超时。

### 模块 06：Skills 管理与存储

**包含什么：** 发现、搜索、仓库管理、下载安装、更新检测、按应用启用、卸载、统一存储 SSOT、存储位置切换、旧数据迁移与备份。

**主要代码：**

- `src/components/skills/`、`src/hooks/useSkills.ts`、`useSkills.helpers.ts`、`src/lib/api/skills.ts`、`src/lib/errors/skillErrorParser.ts`。
- `src-tauri/src/commands/skill.rs`。
- `crates/cc-switch-core/src/services/skill.rs`、`database/dao/skills.rs`、`skill_model.rs`。纯数据模型移入 `skill_model.rs`，服务保留兼容导出。

**重点：** 路径穿越、解压与符号链接、下载失败清理、目录冲突、数据库与磁盘一致性、安装/启用并发、迁移中断恢复、大目录扫描成本。

**边界：** 第一模块只处理启动触发的迁移完整性问题，此处继续覆盖完整 Skills 生命周期。

### 模块 07：提示词、工作区与记忆

**包含什么：** 提示词创建编辑、启用、导入和五客户端原生提示文件同步。已移除客户端的工作区与记忆不作为当前功能入口。

**主要代码：**

- `src/components/prompts/` 中提示词列表、编辑和导入组件。
- `src/hooks/usePromptActions.ts`、`src/lib/api/prompts.ts`、`src/lib/query/prompts.ts`。
- `src-tauri/src/commands/prompt.rs`。
- `crates/cc-switch-core/src/prompt.rs`、`prompt_files.rs`、`services/prompt.rs`。
- `crates/cc-switch-core/src/database/dao/prompts.rs`。

**重点：** 文件范围限制、编码与换行、未保存内容保护、外部修改冲突、错误文件覆盖、空文件与缺失目录、应用间内容隔离。

### 模块 08：会话管理与阅读器

**包含什么：** 多客户端会话发现、解析、分页和搜索；消息阅读、工具调用与图片显示；会话操作；Codex 历史迁移、会话压缩与相关索引。

**主要代码：**

- `src/components/sessions/`、`src/components/sessions/reader/`、`src/hooks/useSessionSearch.ts`、`src/lib/api/sessions.ts`、`src/lib/query/sessions.ts`。
- `src/lib/sessions/turns.ts`、`agentStyles.ts`、`toolSummary.ts`、`search.ts` 保存可复用转换逻辑；阅读器与 Query 均直接依赖 lib 层。
- `src-tauri/src/commands/session_manager.rs`、`src-tauri/src/codex_history_migration.rs`、`src-tauri/src/services/codex_session_compression.rs`。
- `crates/cc-switch-core/src/session_manager/`、`codex_rollout_file.rs`、`codex_state_db.rs`、`services/session_common.rs`。

**重点：** 损坏/截断日志、超大文件、重复消息、时间排序、搜索取消、路径与图片加载、删除/恢复一致性、文件扫描开销。

**边界：** 从会话中提取 token 和费用的计量规则归 09。

### 模块 09：用量、配额与定价

**包含什么：** 代理请求计量、客户端会话用量同步、汇总报表、费用计算、模型价格、汇率、余额与订阅配额、刷新策略。

**主要代码：**

- `src/components/usage/`、`quota/`、`SubscriptionQuotaFooter.tsx` 及账号配额展示。
- `src/hooks/useUsage*.ts`、`useAutoExchangeRates.ts`、`useMoneyFormat.ts`；`src/lib/modelsDev*.ts`、`usageRange.ts`、`managedAccountUsage.ts`。
- `src-tauri/src/commands/usage.rs`、`subscription.rs`、`balance.rs`、`coding_plan.rs`；`services/balance.rs` 及账号服务中的配额读取。
- `crates/cc-switch-core/src/proxy/usage/`、`usage_events.rs`、`usage_script.rs`。
- 核心 `services/session_usage*.rs`、`session_usage_agy/`、`usage_*.rs`、`model_pricing.rs`、`exchange_rates.rs`、`subscription*.rs`、`coding_plan.rs`。
- `database/usage.rs`、`database/sql_helpers.rs`、`database/dao/usage_rollup.rs`、`database/dao/model_pricing.rs` 负责统计及定价读取。
- `pricing.rs` 保存共享价格模型；`services/model_pricing.rs` 仍负责本地文件、价格覆盖和回填编排。
- `database/codex_usage_reset.rs` 承担迁移所需的重置逻辑；会话导入器 `services/session_usage_codex.rs` 随 `session-monitor` 门控。

**重点：** 重复计费、增量游标、缓存 token 归属、缺失价格、精度/溢出、时区和日期边界、配额刷新失败、聚合查询性能。

### 模块 10：授权与账号管理

**包含什么：** OAuth 授权发起、回调、轮询、取消；token 刷新；账号增删和切换；凭据持久化；官方订阅账号与供应商绑定。

**主要代码：**

- `src/components/settings/auth/`、`AuthCenterPanel.tsx`；`src/components/providers/forms/` 中 OAuth/官方账号组件及 `hooks/useManagedAuth.ts`。
- `src/lib/authBinding.ts`、`authLoginLink.ts`、`src/lib/api/auth.ts`。
- `src-tauri/src/commands/auth.rs`、`codex_oauth.rs`、`agy_account.rs`、`claude_account.rs` 及 `dsh.rs` 中授权相关命令。
- `src-tauri/src/services/agy_account/`、`dsh_account.rs`；核心 `services/claude_account/`。
- 核心 `proxy/providers/` 中认证管理器，以及 `services/provider/` 中登录、OAuth 切换和官方账号绑定逻辑。
- 核心 `auth_state.rs` 定义认证访问接口；GUI `frontend_hooks.rs` 与 CLI 的认证适配保留各自宿主职责。授权变化经共享动作入口刷新账号与供应商快照。

**重点：** state/回调校验、凭据泄露、刷新并发、过期时间、账号错绑、取消后回调、登录切换回滚、存储失败。

### 模块 11：设置、导入导出与云同步

**包含什么：** 应用设置、目录覆盖、环境冲突管理；备份列表与恢复；导入导出；WebDAV/S3 手动与自动同步；同步快照、版本及冲突处理。

**主要代码：**

- `src/components/settings/` 中设置/目录/数据/网络相关页；`src/components/env/`。
- `src/hooks/useSettings*.ts`、`useDirectorySettings.ts`、`useNativeConfigPath.ts`、`useBackupManager.ts`、`useImportExport.ts`；`src/lib/api/settings.ts`、`env.ts`。
- `src-tauri/src/commands/settings.rs`、`config.rs`、`env.rs`、`import_export.rs`、`webdav_sync.rs`、`s3_sync.rs`、`sync_support.rs`；`app_store.rs`。
- `src-tauri/src/services/backup_storage.rs`、`config.rs`、`env_checker.rs`、`env_manager.rs`、`env_manager/`。
- 核心 `settings.rs`、`services/auto_sync.rs`、`sync_protocol.rs`、`webdav*.rs`、`webdav_sync/`、`s3*.rs`、`sync_utils.rs`。
- `AutoSyncDatabaseObserver` 将数据库脏通知转交同步服务；其装配归模块 01，数据库只依赖 `database/changes.rs` 中的接口。

**重点：** 设置局部更新丢字段、路径覆盖、备份覆盖、远端数据校验、同步重入、取消/回滚、时钟与版本冲突、认证信息泄露。

**边界：** 更新安装与重启流程归 01；SQLite 备份原语与 Schema 兼容归 14；工具安装设置归 12。

### 模块 12：CLI、工具发现与进程生命周期

**包含什么：** 独立 cc-switch CLI；客户端工具发现、版本检测、安装、升级、卸载；终端启动；进程超时取消；Windows/WSL/macOS/Linux 命令与路径适配；相关插件入口。

**主要代码：**

- `crates/cc-switch-cli/src/main.rs`、其 `Cargo.toml` 和依赖的核心公开接口。CLI 的供应商列表命令显式调用 `ProviderService::refresh`；故障转移通过核心执行器与 GUI 共用实现。
- `src/components/apps/`、设置目录中的 `Tool*.tsx`、`TerminalSettings.tsx`；`src/lib/api/vscode.ts`。
- `src-tauri/src/commands/misc.rs` 中工具入口，以及 `tool_discovery.rs`、`tool_lifecycle_command.rs`、`tool_node_requirement.rs`、`tool_process.rs`、`tool_uninstall.rs`、`tool_version.rs`。
- `src-tauri/src/commands/plugin.rs`、`claude_plugin.rs`；核心 `process_util.rs`、`background.rs`、`services/provider/codex_terminal.rs`。

**重点：** 命令参数转义、PATH 和版本识别、进程树回收、安装并发、卸载范围、取消后残留、CLI 退出码、平台差异。

### 模块 13：托盘、深链接与平台集成

**包含什么：** 原生托盘菜单、托盘独立面板、状态刷新和操作；URL 导入解析及确认；单实例唤起、窗口重建、轻量模式、自启、窗口状态与平台修复。

**主要代码：**

- `src/tray-panel/`、`src/components/DeepLinkImportDialog.tsx`、`src/components/deeplink/`、`src/lib/api/deeplink.ts`。
- `src-tauri/src/tray.rs`、`tray_panel.rs`、`tray_compat.rs`、`deeplink/`、`lightweight.rs`、`auto_launch.rs`、`linux_fix.rs`、`window_fit.rs`。
- `src-tauri/src/commands/deeplink.rs`、`lightweight.rs`；`lib.rs` 中对应系统事件适配。

**重点：** 窗口销毁/重建竞态、主线程约束、菜单旧状态、深链接参数与大小限制、重复导入、焦点和任务栏状态、跨平台一致性。

**边界：** 启动显示策略与退出编排归 01；深链接确认后的业务写入归对应业务模块。

### 模块 14：数据库、Schema 与持久化基础

**包含什么：** SQLite 初始化、连接锁、Schema 升级、迁移原子性、DAO 通用约束、备份/恢复机制、损坏数据处理、索引与查询计划。

**主要代码：**

- `crates/cc-switch-core/src/database/mod.rs`、`schema.rs`、`migration.rs`、`backup.rs`、`tests.rs`。
- `crates/cc-switch-core/src/database/dao/` 全部 DAO 的事务、约束、序列化与并发契约。
- `database/changes.rs`、`usage.rs`、`sql_helpers.rs`、`codex_usage_reset.rs`、`icon_sanitizer.rs` 及相关数据库集成测试。
- `Database.conn` 和日志计数缓存限制为 core crate 内可见；数据库观察者通过构造函数注入，桌面定价查询改用 `Database::list_model_pricing`。
- `store.rs` 的状态生命周期与模块 01 共同核查；`app.rs`、`skill_model.rs`、`pricing.rs`、`proxy_config.rs` 是持久化与业务共用的类型入口。

**重点：** 部分提交、数据库版本过新、重复执行迁移、锁中毒/死锁、备份时并发写入、非法 JSON、整数与空值转换、索引缺失。

**边界：** 各 DAO 的业务正确性随所属业务先审；本模块集中核查跨表一致性和共用持久化保证。

### 模块 15：客户端原生配置与文件写入基础

**包含什么：** 各客户端原生文件格式；JSON/JSONC/TOML/env 读写；字段补丁与合并；原子落盘、写前快照、外部编辑冲突、崩溃恢复和文件路径解析。

**主要代码：**

- `crates/cc-switch-core/src/live/`，包括 engine、patch 及文件写入支持。
- 核心 `app_config.rs`、`config.rs`、`jsonc_document.rs`；共享应用身份移至 `app.rs`，模型命名语义移至 `model_names.rs`。
- 核心 `codex_config.rs`、`opencode_config.rs`、`agy_config.rs`、`dsh_config.rs`；Claude 配置由 `config.rs` 与 `live/project/claude.rs` 管理。
- `services/provider/live.rs` 及各客户端 direct/editor 适配中的文件层职责；`mode/operation.rs` 中操作恢复协议；`src/lib/errors/liveEditConflict.ts`。

**重点：** 未知字段/注释丢失、读取失败被当空配置、格式转换、并发覆盖、权限和软链接、跨文件非原子更新、备份与恢复覆盖外部改动。

**边界：** 供应商选择规则归 03、代理接管决策归 04；本模块验证这些操作共用的文件安全与兼容保证。

### 模块 16：通用界面、国际化、诊断与构建测试

**包含什么：** 公共 UI、主题、可访问性和输入交互；翻译资源；错误解析、日志与诊断；公共工具；测试基础设施；类型检查、构建、桌面权限、打包及发布脚本。

**主要代码：**

- `src/components/ui/`、`common/`、`theme-provider.tsx`、`FrontendErrorBoundary.tsx` 等公共组件；`src/index.css`。
- `src/i18n/`、`src/icons/`、`src/assets/`、`src/whats-new/`；`src/utils/` 和未归入业务模块的 `src/lib/` 公共工具。
- `src/lib/frontendLogger.ts`、`toast.ts`、`platform.ts`、`inputModality.ts`；核心 `error.rs`、`events.rs`、`auth_state.rs`、`runtime_hooks.rs`、`frontend_apps.rs`、`model_capabilities.rs`；GUI `panic_hook.rs`、`frontend_hooks.rs`。
- `tests/`、`src-tauri/tests/`、源码内测试、MSW 夹具与测试配置。
- `package.json`、锁文件、`tsconfig*.json`、`vite.config.ts`、`vitest.config.ts`、Tailwind/PostCSS 配置、各 `Cargo.toml`、`rust-toolchain.toml`。
- `src-tauri/build.rs`、`tauri*.conf.json`、`capabilities/`、`wix/`、平台 manifest/Info.plist，`scripts/`、`.github/workflows/`、`flatpak/`。

**重点：** 键盘和焦点、错误提示可操作性、翻译缺失、日志敏感信息、测试隔离、mock 与真实协议偏离、feature 组合、依赖锁定、打包权限和升级资源。

## 四、覆盖边界与进度要求

- `node_modules/`、`target/`、`target-gnu-check/`、`dist/` 为依赖或生成产物，不逐文件审查；需要核实依赖契约或构建结果时定向查阅。
- `src-tauri/gen/` 等生成内容检查其来源和使用方式，修复优先落在源配置。
- `docs/`、README 和变更记录用于核对行为约定；代码行为变化时同步受影响的文档。
- 历史审查文档、临时脚本及编辑器配置先确认用途，不因文件存在就视为产品运行路径或删除对象。
- 模块范围表不是文件穷举表。进入模块时补充实际文件清单；发现未归属源文件时先补入相应模块，防止遗漏。
- 审查结论需区分发现、修复和实际验证结果；只报告本次执行过的检查及未验证的平台范围。

## 五、维护与验证

当前职责、共享接口及未解决的一致性边界统一维护在[架构说明](docs/architecture.md)，测试入口统一维护在[测试指南](docs/testing.md)。模块编号用于定位代码，不代表该模块已经审查、修复或通过全部验证。
