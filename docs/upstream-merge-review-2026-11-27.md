# 源仓库近期修复与优化提交合并评估

评估日期：2026-11-27。对象为本机 `D:\tmp\cc-switch\cc-switch-source`，目标为当前 `feat/five-apps-20261126` 分支及其未提交工作区。

**建议按问题提取修复，不整体合并源仓库 main，也不按提交日期批量 cherry-pick。** 优先补齐故障转移竞态、MCP/SQL 导入校验、流式统计边界、Codex 协议转换和 OpenCode 新版兼容。性能优化优先考虑用量统计查询、QuickJS 单次求值和五应用扫描共享游标；连接池、数据库 WAL/缓存单独评估。

**支持范围已按用户确认修正：xAI、Copilot 均不再支持。** 先前复审把残留实现当成受支持功能，据此提出的相关合并建议不成立，现已撤回。合并价值须以当前支持范围及实际需求为前提，不能由文件、类型或历史分支仍然存在推定。本次不恢复这两类服务，也不修改其残留代码。

本次及二轮复审只新增、更新此文档。没有修改业务代码、执行合并、切换分支、更新依赖或访问真实账号。本报告是提交差异与当前实现的静态对照，**不表示问题已经运行复现、上游测试已经在本分支通过，或性能收益已经测得**。

## 1. 分析基线与范围

| 项目 | 本次基线 |
| --- | --- |
| 源仓库 | `D:\tmp\cc-switch\cc-switch-source`，`main`，工作区干净 |
| 源仓库 HEAD | `078ea4a859c4372d6846383085563b009831aea4`，`fix(env): delete only the selected line of a shell config (#10025)` |
| 源提交时间 | 2026-11-27 16:56:40，UTC+08:00 |
| 当前仓库 HEAD | `c2e71905eb80b1761ace2b62a980cd2e88c3a7b7` |
| 当前分支 | `feat/five-apps-20261126` |
| 当前工作区 | 开始评估时有 850 个已跟踪文件修改、176 个删除、71 个未跟踪条目；必须把这些改动纳入覆盖判断 |
| 实际保留客户端 | Claude、Codex、OpenCode、DeepSeek Harness、Antigravity；见 [AppType](../crates/cc-switch-core/src/app.rs) |
| 明确排除的服务 | xAI、Copilot（用户已确认不再支持）；包括其认证、迁移、额度、托盘、专用转换及路由功能 |
| 历史同步锚点 | 当前 `a32bce0d` 对应源仓库 `09acca049`，均为 `build: line-tables-only debug info for local dev and test builds (#9568)` |
| 扫描范围 | 源仓库 `09acca049..078ea4a85`，294 个提交，主要覆盖 2026-11-16 至 2026-11-27 |
| 提交分类 | fix 138、perf 14、refactor 54、feat 32、test 19、chore 21、docs 15、ci 1 |

两边历史存在重新落地的提交，不能仅凭 SHA 不同认定“没有合过”。上述锚点的 `src-tauri/Cargo.toml` 补丁，经 `git patch-id --stable` 核对一致：`f079b24a584abc0c4bbfe818e94fbe3bddc18e3c`。两仓库最近的共同提交 SHA 仍停在 2026-11-02，因此直接拿共同祖先后的全部提交当作新增清单会过度扩大范围。

本次扫描全部 294 个提交标题与涉及文件，再重点核查适用于当前产品的修复和优化补丁及当前对应实现。下文是经过筛选的决策清单，不是对 294 个提交逐条做完运行验收。未执行 fetch；“最近”指上述本地源仓库快照，不代表 GitHub 远端此刻状态。时间以提交记录为准，不根据作者日期推断依赖顺序。

**工作区并行更新说明**：收尾时发现工作区指纹与开始时不同，新增了 `src/i18n/language.ts` 等语言模块及测试，并出现前端格式化变化；这些不是本次分析操作写入的。本次没有回退或覆盖它们。2026-11-27 19:46（UTC+08:00）重新核对后，语言范围已为中文/英文，相关判断已更新，五应用枚举和主要后端缺口仍成立。因此本报告依据逐文件读取时的状态，不能当作冻结工作区上的完整验证结果；实施前应重新确认最新改动。

### 二轮复审结论（2026-11-27）

二轮核对时，两边 HEAD 和目标分支未变，源仓库仍干净；目标工作区继续有外部变化。复审重新追踪了实际调用、落盘及显示路径，不以“缺少某个上游函数名”作为未修复的充分依据。以下修订已经同步到正文，不是额外的一套冲突结论。

| 修订 | 二轮结论 |
| --- | --- |
| 补回协议修复遗漏 | `8162adb3c` 的 Gemini 长历史签名缓存适用于现有 Claude→Gemini 和 Codex→Gemini，列为 P1。 |
| 补回工具链遗漏 | `3cf5865ca` 的通用 Unix 探测修复可提取，不能因提交同时改 Pi 插件而全部排除。 |
| 补回可选体验修复 | Linux 终端 `51657ffae`、macOS 托盘退出 `2a0f43331`，以及 Skills 非 ZIP 错误分类。按对应平台与使用频率安排。 |
| 撤回超出支持范围的建议 | Copilot 迁移 `9431a6bc9`、xAI host 修复建议 `37e3eabfe`、xAI 托盘额度 `0e8a584b6` 均移出推荐及实施批次，原因是对应服务已不支持。 |
| 收窄原来的缺口描述 | MCP 已有基础 schema 校验；Chat 非流式正文已支持 text 数组；CONNECT 已按数字解析状态码。这些都只补剩余行为。 |
| 下调一项候选 | `f29d0804b` 在当前卡片上未确认显示串号：虽然查询可能读到缓存，账号副标题和额度组件有类型分支保护。仅作可选组件整理，不列入缺陷批次。 |
| 确认性能依据与实施顺序 | 中位数的两次扫描已核对到供应商/模型统计调用点；P0 导入和路由一致性先于 P1 小修复，修正首轮顺序与优先级不一致的问题。 |

这里的“确认缺口”表示代码路径及源补丁可对应，置信度高于仅按标题筛选；仍需实施阶段的回归用例。平台行为、真实端点和性能收益尚未实测。

### 目录与职责映射

| 上游位置 | 当前落点 | 移植要求 |
| --- | --- | --- |
| `src-tauri/src/proxy/*` | `crates/cc-switch-core/src/proxy/*` | 保留核心库不依赖 Tauri 的边界，以及桌面/CLI 共用的 runtime hooks |
| `src-tauri/src/database/*` | `crates/cc-switch-core/src/database/*` | 保留本地迁移、导入回滚、权限与事件通知逻辑 |
| `src-tauri/src/services/usage_stats.rs` | 实现在 `crates/cc-switch-core/src/database/usage.rs` | 当前 `services/usage_stats.rs` 只是兼容导出，不能向其中直接粘贴上游实现 |
| `src-tauri/src/services/session_usage*.rs` | `crates/cc-switch-core/src/services/session_usage*.rs` | 只适配五个保留客户端的扫描链路 |
| `src-tauri/src/commands/*`、系统账号适配 | 大部分仍在 `src-tauri/src/*` | 需逐文件确认；例如 Agy 账号管理仍在桌面层，不能因为核心库没有同名文件就判为已移除 |
| `src/*` | `src/*` | 当前 UI、语言和应用列表已精简，优先移植行为与回归用例 |

优先级含义：P0 表示本轮最先处理的数据一致性/导入边界问题；P1 表示明确影响现有功能，建议随后移植；P2 表示收益有条件、成本较高或优先级较低。优先级不是漏洞评级。

## 2. 最值得先合并的修复

以下项目已经在当前代码中找到对应缺口。表中的“部分覆盖”表示应只补缺失部分。

| 优先级 | 源提交 | 建议与当前证据 | 成本 |
| --- | --- | --- | --- |
| P0 | `e8eb86d30`，#9946，旧请求不覆盖手选路由 | **优先移植路由版本校验。** 当前 `proxy/forwarder.rs:517` 仍直接写 `current_providers`，随后异步执行 failover hook；`mode/controller.rs:1697` 的 `record_failover_route` 没有请求开始时的路由版本参数。串行锁只能串行执行，不能识别结果已经过期。 | 高，跨路由、DB、hooks、桌面/CLI |
| P0 | `b1d7598d7`，#9831，MCP 深链接完整展示与危险字段拦截 | **移植深链接专用限制与完整预览。** 实际链路是 `deeplink/mcp.rs` → `McpService::upsert_server` → `validate_server_spec` → 写入计划，已有类型、HTTP(S) URL、请求头和 NUL 校验；但未限制危险扩展字段、凭据变量引用及 UNC 工作目录。Claude 写出仍克隆整份 spec；`transport_spec` 只用于比较，不是持久化过滤器。确认页也未完整展示扩展字段。 | 中 |
| P0 | `31575905b`，#9832，SQL 导入限制 | **补 trigger/view 禁止与暂存库 schema 检查。** 当前 `database/backup.rs` 已有大小、执行时间、SQLite 资源预算，但 `import_authorizer` 和 schema 校验未禁止这些对象。二进制 `.db` 恢复走 Backup API，不执行外部 SQL，必须同时经过 schema 检查。 | 中 |
| P1 | `d2bb4c8d4`，#9835，安全边界合集 | **提取 CONNECT 读取限制、凭据解码及深链接风险提示增量。** 当前 `hyper_client.rs:628` 的 `read_line` 无字节/行数限额；已有数字状态码匹配，但未验证 HTTP 状态行前缀，提前 EOF 会被读头循环当成结束。`deeplinkRisk.ts` 还缺解释器内联执行和部分环境变量风险规则。DB/SQL 私有权限和主要备份路径校验已有实现。 | 中，必须拆分提交 |
| P1 | `f2b3c12eb` → `84e4612c7`，#9846/#9867，SSE 收集与逐事件限额 | **组合提取，不能只拿第一笔。** 当前 `response_processor.rs:417` 只留最后 256 个事件，会丢最前面的 usage；`sse.rs:104` 以整个 incoming chunk 计限额，可能误拒包含很多合法小事件的大块数据。 | 中 |
| P1 | `bf4cc69fe`，#9930，全局代理原子发布 | **只补同一快照读取 URL 与 Client 的接口及调用链。** 当前已有 `CLIENT_CONFIG_LOCK`、桌面设置串行锁和应用失败回滚；但 `forwarder.rs:2761` 先读 proxy URL，再另取 Client，切换可发生在两次读取之间。 | 中 |
| P1 | `dce5608be`，#9829，用量脚本防护 | **提取活跃代理监听器登记、自转发阻断及 URL 校验差异。** 当前 `usage_script.rs:255、298、441` 已禁止重定向、限 2 MiB、单遍转义变量；发送前没有源提交所加的活跃监听器自转发检查。 | 中 |
| P1 | `d15cb087c`，#8659，Codex→Anthropic 不透传 strict | **小而明确，列入 P0 之后的快速修复批次。** 当前 `transform_codex_anthropic.rs:481` 仍把 OpenAI 的 `strict` 字段复制到 Anthropic 工具定义。上游修复针对严格校验的兼容端点。 | 低 |
| P1 | `82c29d09a` → `148c1005b`，Responses message/reasoning ID | **流式与非流式一起移植。** 当前 Chat 流仍构造 `rs_{response_id}`、`{response_id}_msg`，Anthropic 流仍为 `{response_id}_msg_{index}`；上游分别统一 `msg_` 消息 ID 和带输出索引的转换 reasoning ID。 | 低至中 |
| P1 | `4c9e5c3b0`，OpenCode 2.4 的 `session_part` | **建议移植，属于保留客户端的直接兼容修复。** 当前 `session_manager/providers/opencode.rs:454` 的 V2 reader 只从 `session_message.data` 取正文，没有 `session_part` 路径。 | 中 |
| P1 | `3c49b5cdc`，OpenCode 安装发布线识别 | **建议移植，覆盖查版本、升级、卸载及提示命令。** 当前 `commands/tool_version.rs:104` 和 `misc.rs:630` 仍以 `opencode-ai` 处理；缺少上游对 `@opencode/cli` 的识别和同发布线选择。 | 中至高 |

### 2.1 这些修复落地时必须保留的本地行为

- **故障转移版本**：将请求开始时的版本一路传到 `runtime_hooks::FailoverRequest` 及核心控制器；手动切换、模式变更等会改变路由的操作都要推进版本。当前已有 Stack 请求隔离，不能因改用上游代码而丢失。还要校验桌面与 CLI 两种 hook 实现，不能把上游 `AppHandle` 依赖重新带回核心库。
- **MCP**：在深链接边界拒绝隐式读取宿主凭据或执行 helper 的字段，前端完整展示实际会保存的字段；不要直接收紧所有手动编辑入口。保留当前“同 ID 保留已有配置，仅合并应用范围”的提示与语义。验证覆盖嵌套 `server`、大小写、UNC、环境变量展开，以及合法静态 headers、URL 百分号编码、普通本地 cwd；不能只加 UI 警告。`d2bb4c8d4` 的风险提示规则可同批补齐，但提示不能代替后端拒绝。
- **SQL**：保留本地 256 MiB 读取限制、30 秒/指令预算、临时库验证和失败回滚。检查必须在 SQL/WebDAV 导入和 `.db` 恢复的暂存阶段生效，并先于迁移及替换真实库。上游新的禁止规则需要与本地导出 trigger 的测试分开处理：应用导入可以拒绝这些对象，不应随意删掉底层 SQL 导出能力的独立测试。
- **CONNECT**：保留当前不绕过用户代理的失败策略；补齐状态行/头部总字节数、头行数、HTTP 前缀、提前 EOF 和编码凭据测试。状态码数字匹配不等于完整状态行校验；`garbage 200` 和缺终止空行应失败。不要用整文件替换抹掉本地取消连接的收尾逻辑。
- **SSE**：提取“保留头部与尾部事件”和“按事件判断大小”的逻辑，保留当前限额策略。旁路日志收集遇到超大事件可放弃该事件的统计；协议转换链路不能因此静默丢掉模型正文或工具参数。上游 collector 限额与本地 16 MiB 不同，不能不经评估直接放大。
- **代理配置**：本地 `commands/global_proxy.rs:30` 已有保存失败补偿，比只串行写 DB/运行态更完整。优先增加同批读取接口，不回退现有补偿。
- **OpenCode 会话**：除 reader 外，同步适配 `content.rs`、`opencode_blocks.rs` 的长内容定位和删除清理；上游 `search_revision.rs` 属于当前没有的全文索引模块，不要为了移植它恢复整个模块。
- **OpenCode 安装**：保留当前用户选择安装位置、锁和 Windows npm 安装参数。必须区分 npm 包、原生安装、桌面附带 CLI，避免升级命令改变当前安装线。版本与包名事实以本次源提交为依据，执行实际升级前仍应再核对目标发行物。

## 3. 随后值得移植的功能修复

| 优先级 | 源提交 | 结论、适用性与边界 |
| --- | --- | --- |
| P1 | `1963028ee`，Codex 工具 schema 经 Chat 网关到 Gemini | 值得移植。当前 Chat schema 清洗没有移除 schema 节点的 `encrypted` 标记，也没有 Gemini 专用非字符串 enum 适配。当前新增的 `transform_codex_gemini.rs` 是原生 Gemini 协议路径，不能替代 Chat 网关修复。只按目标模型适配，保留名为 `encrypted` 的真实参数和 default/examples 中的字面数据。 |
| P1 | `c73d8a48c`，#9554，扁平工具名碰撞 | 值得移植。当前 `transform_codex_chat.rs:112` 遇到已经出现的 chat name 就直接跳过，命名空间工具与同名普通工具可能丢失。移植稳定去重命名及双向映射，联动 Anthropic 转换；需验证历史回放和工具结果能还原身份。 |
| P1 | `0c1121ce3`，Chat 分段 text/thinking | **部分覆盖，补分段 thinking 和流式解析。** 当前 `chat_message_to_response_output_item` 已解析非流式 `content` 中的 text/output_text/refusal 数组；`chat_reasoning_text` 仅从专用 reasoning 字段或字符串 think 块取值，漏掉 `content` 中的 thinking 段。上游同时修改 common、Claude/Chat、Codex/Chat 流与非流处理，应验证这组路径，而非仅根据 helper 缺失判断。 |
| P1 | `8162adb3c`，#9009，Gemini 长工具历史签名 | **二轮补入，建议与协议修复同批。** 当前 `GeminiShadowStore` 只留每会话最近 64 个完整 turn，淘汰时没有独立签名缓存；`transform_gemini.rs:85` 对整段历史补占位签名。提取按 provider/session/call ID 精确匹配的有界签名缓存、避免工具 turn 按位置错配、仅对当前轮补缺失签名。保留的 Claude→Gemini 与 Codex→Gemini 共用此实现，后者也需回放测试。 |
| P1 | `1fba4e16c`，#8825，原生 Responses 并行工具结果分组 | 值得移植。当前没有 `transform_codex_responses_reorder`。保留官方认证透传例外；按用户轮次重排完整调用组，不能跨用户消息移动或把缺失结果的调用强行凑成完整组。 |
| P1 | `46726137c`，新模型工具轮次保留 adaptive thinking | 值得提取历史校验例外。当前 `thinking_optimizer.rs:96` 已覆盖更广的默认 adaptive 模型，但 `transform_codex_anthropic.rs:309、337` 仍基于工具历史判断关闭 thinking，缺少上游的 `accepts_unsigned_tool_history`。不要用上游更窄的模型名单覆盖本地名单；具体模型行为需协议回归及必要的真实端点验证。 |
| P1 | `d4290287b`，#9952，TPS/TTFT 统计 | 部分覆盖，仍值得补齐。当前有首次输出检测，但没有源提交对转换流的区分和完整语义帧规则；`database/usage.rs:157` 的速度样本 SQL 未限定成功状态及异常 TPS 上限。应同时修改核心统计、前端 `usage/format.ts` 和详情展示，防止两侧口径不一致。 |
| P1 | `9967927cb`，Claude 回复关联实际请求 | 值得移植。当前 `session_usage.rs:338、846` 仍从 `parentUuid` 链解析请求起点，没有 `requestRef`/`api_request` 对应关系。重试或长时间后重发时，旧起点会污染延迟和速度。保留游标之前记录的检索与无法匹配时的回退。 |
| P1 | `c131f68e9`，#9249，模型前缀导致的跨来源重复计费 | 值得移植。当前 `database/usage.rs:659` 的匹配仍直接比较模型名。上游补了 `provider/model` 前缀归一化；保留现有应用、时间、token 等其他去重条件，并测试不同供应商同后缀模型不能被过度合并。 |
| P1 | `d010759ec`，#9539，Skills ZIP 可执行权限 | 值得移植。当前 `services/webdav_sync/archive.rs` 没有保存/恢复普通文件 Unix mode 的逻辑，脚本跨设备同步后可能不可执行。移植安全 mode 清洗及上传/恢复两端，保留本地目录逃逸、符号链接和解压预算保护；Windows 保持兼容。 |
| P1 | `6313363ed`，Skills 下载按停滞判断超时 | 值得移植。当前 `services/skill.rs:890、2885` 等入口仍是总计 60 秒超时。源提交把持续传输与无进度停滞分开，适合大仓库慢链路。保留总时间兜底、压缩/解压大小上限和分支回退的统一预算。 |
| P2 | `8e043d9ea`，同仓库同安装名只展示一项 | 值得做的小修复。当前 `skill.rs:2883` 扫描后直接返回，安装则按末级目录名复用。只在发现列表按确定性规则去重，不能从更新扫描中删掉原始路径信息。 |
| P2 | `49e5d9a00` → `21d083cec`，Unix 安装探测和子进程环境 | 值得作为 macOS/Linux 适配批次。当前有登录 shell PATH 读取，但没有后者的受控工具链环境集合；前者针对安装枚举，与运行时命令不是同一入口。仅传递上游明确筛选的工具链变量，不全量复制 shell 环境；保留当前 deadline 和测试 home 隔离。 |
| P1（Unix） | `3cf5865ca`，#9964 的工具发现部分 | **二轮补入。** 当前 `tool_discovery.rs:1030` 直接执行 `command -v`，取第一行绝对路径；启动脚本打印路径会干扰解析。`locate_default_tool:1351` 的 fallback 返回 canonical 路径，可能丢掉 multicall/shim 的启动名。提取输出标记、launcher/real 分离与 Unix pnpm 路径补充；保留本地 `is_file`、进程 deadline、Windows 查找保护。Pi 插件校验部分仍不适用。 |
| P2 | `51657ffae`，#9987，Linux 系统终端入口 | **二轮补入，低成本。** 当前两套候选列表仍在 `commands/misc.rs:2296、2737`，均缺 `x-terminal-emulator -e`、`xdg-terminal-exec --`。提取共用候选表，同时覆盖普通终端与会话恢复启动，测试用户首选项和参数顺序。当前没有上游 `terminal_launch.rs`，不要直接新增同名文件照搬。 |
| P2（macOS） | `2a0f43331`，退出后保留菜单栏图标位置 | **二轮补入，平台限定。** 当前 `lib.rs:1140` 在所有平台退出时隐藏托盘；上游在 macOS 跳过显式移除。只适配退出/重启清理分支，保留 Windows 清理残留图标行为。菜单栏位置保存的实际效果需 macOS 验证。 |
| P2 | `a5fd82b6a`，Skills 下载非 ZIP 错误分类 | **二轮补入，仅补结构化错误。** 当前 `skill.rs:3560` 仍直接传播 ZipArchive 错误，缺 `ARCHIVE_INVALID`；但 SkillsPage 已读取并展示 `repoFailures`，不能说当前完全把失败当作 0 条。可与下载超时批次一起完善错误码/中英提示，并检查添加仓库的实际入口，保留现有失败列表。 |
| P2 | `112bf52cd`，#9979，Agy 钥匙串写后重复核验 | 值得小范围移植，主要适用于当前支持钥匙串切换的平台。当前桌面 `services/agy_account/mod.rs:357` 写后只核验一次。源提交只重读、不再次覆盖登录，能避免把短暂读取失败直接当作切换失败。 |
| P2 | `a48f9d6e5`，热力图开头月份标签重叠 | 值得顺手移植，成本低。当前 `UsageHeatmap.tsx:202` 第一列无条件标月份；上游限制首标签与下一个月份标签的间距。检查跨月、跨年和不足三列的起始区间即可。 |

### 3.1 有价值，但应先用针对性用例核实的候选

| 提交 | 为什么暂不放入第一批 |
| --- | --- |
| `347348716`，#9638，legacy Codex fork 从 idle parent 恢复 | 当前 `session_usage_codex.rs:2093` 已有 `verified_ordinal_fork`，还增加了本地 `verified_bounded_fork`；上游新补的是 legacy provenance、父文件完整性和快照稳定性判定。建议先移植能区分这几种格式的 fixture，再补剩余 legacy 缺口。整文件替换会覆盖本地 bounded-history 修复。 |
| `b7d2ad94b`，#9848，Codex OAuth 路由 prompt caching | 上游对两个精确模型 ID 把 instructions 移入首条 developer message；当前没有该分支。可以单独评估，但属于特定模型/端点行为，不能推广到全部 Responses 请求。本次未验证真实端点缓存命中和提示语义，因此不作为已确认性能收益。 |
| `d910a92d8`，#9099，OpenCode Desktop 自带 CLI 升级保护 | 与 `3c49b5cdc` 联合复核，不能假定“有 sidecar 探测”就覆盖升级。当前 `windows_preferred_tool_executable` 已识别 sidecar，`resolve_update_command` 也拒绝未托管原生文件；但缺上游显式 `source == desktop` 分支。先检查安装识别、锚定命令优先级及脚本 launcher fixtures，再补确有遗漏的路径，避免重复已有原生文件保护。 |
| `9455eb319`，#9113，AppImage 调用宿主打开程序 | 当前配置目录、Skills 目录及链接仍直接走 opener，未见源提交的 `host_open` 适配；若继续发布 Linux AppImage，值得单独评估。需核对当前打包环境和实际启动路径，再提取所需环境恢复逻辑；不为此恢复 Hermes 等已移除入口。当前 Windows 静态审查未复现平台问题。 |

## 4. 性能优化：收益与成本分开判断

| 优先级 | 提交 | 当前证据与建议 | 主要验证点 |
| --- | --- | --- | --- |
| P1 | `3b08b1162`，#9825；`aaf6ee963`，#9920 的统计部分 | **性能批次中最值得先做。** 当前 `database/usage.rs:207` 在 SQL 中排序，逐行创建分组字符串；供应商统计 `1804–1817`、模型统计 `2022–2033` 分别调用精确/估算中位数查询，已确认两次扫描。移植一次扫描分流、选择中位数和复用分组键；检查缺失的 `idx_model_pricing_nocase`。不要重复加本地已有的 request log 索引。 | 同一数据集结果完全一致；奇偶样本、空组、模型/供应商筛选、query plan；记录耗时和分配量 |
| P1 | `e7915ecfa`，#9817，QuickJS 一次求值 | **值得移植单 runtime/单 eval 思路。** 当前 `usage_script.rs:71、148` 已在 blocking worker 执行，但仍创建两份 runtime 并重复 eval。源提交进一步复用配置和 extractor，避免状态型脚本两次求值不一致。 | 保留 JS 时间/内存/栈限制；网络等待后重置执行期限；大量慢查询时 blocking worker 占用可控 |
| P1 | `23b937b20`，#9818，共享游标与并行扫描 | **先共享游标，再评估受限并行。** 当前 `session_usage.rs:121` 串行扫描五个客户端，各解析器自行预取游标。共享快照可直接减少重复读；并行可缩短扫描时间，但可能争用同一 DB 锁。 | 五应用批次总耗时、DB 锁等待、峰值线程/内存、任一解析器失败仍汇总其他结果、通知只在合适时机发出 |
| P2 | `83a3161fc`，#9914，Claude 原生转发连接复用 | **有价值，单独立项。** 当前 raw header 保真路径没有源提交的 `upstream_pool`。收益主要是同上游连续请求减少连接/握手；上游新增大型连接生命周期模块，绝非小参数优化。 | 请求体已发出后不可盲目重放；完整消费才能回池；取消、超时、代理切换、连接关闭、睡眠唤醒；按目标和代理隔离 |
| P2 | `4e10bd857`，#9820，WAL 与代理热读缓存 | **只在确认 DB 瓶颈后适配。** 当前数据库没有这套 `ProxyReadCache`；上游同时更改连接所有权、update/rollback/authorizer hooks 和 backup replacement，且包含 Tauri runtime 回退。不能仅复制几个 PRAGMA。 | 桌面与 CLI 多进程写入的缓存失效、导入/恢复、迁移、页大小差异、WAL/SHM 权限、备份一致性；保留核心库无 Tauri 依赖 |
| P2 | `fcff029cf`，#9821，减少请求体复制 | **只提取当前路径仍有的复制与重试优化。** 当前 `forwarder.rs:1388、1391` 仍克隆 body，但上游依赖拆分后的 RequestBody/RetryBody，同时处理当前不存在的 privacy 模块。先测大工具历史请求，再决定哪些复制可以消除。 | 首次请求、同家重试、故障转移、模型重映射和工具字段保留；不得把第一次尝试的变换污染后续尝试 |
| P2 | `2ccbfe2b3`，#9886；`5adff00c1`，#9418 的 UI 增量 | **主要取“先显示安装状态，版本结果逐项到达”。** 当前 `commands/misc.rs:248` 已有 `buffered(4)` 并发版本探测，因此不需要照搬无上限 `join_all`。本地 UI 仍等待版本批次结果，轻量安装报告可以另外评估。 | 首屏等待、慢/失败工具不挡其他行、缓存写回不覆盖新请求、安装/卸载期间的状态一致性 |

`aaf6ee963` 的 HTTP 辅助客户端懒初始化已经有本地实现，不应与其统计优化一起整笔套用。`bf4cc69fe` 的状态代际/快照可以参考它的最终结构，但保留本地仅为已使用客户端重建的策略。

对于 `4e10bd857`，尤其需要额外考虑：上游进程内 update hook 不会自然观察另一个 CLI 进程的写入。当前有独立 CLI，因此任何跨请求配置缓存都要有跨进程失效方案，不能只验证同一 `Database` 实例上的 mutation。

## 5. 已覆盖或不值得重复合并的提交

这里的“覆盖”指表中列出的具体行为，不表示整个上游提交与当前文件逐字一致。

| 提交 | 当前状态与决定 |
| --- | --- |
| `078ea4a85` + `01441512b`，Shell 按选中行删除、保留换行 | **无需重复移植。** 当前 [env_manager/snapshot.rs](../src-tauri/src/services/env_manager/snapshot.rs) 按文件分组，`split_inclusive` 保留换行，并校验扫描后的名字/值及整个文件状态；有 `deletes_only_selected_occurrence_and_restores_exact_content_once` 回归用例。当前机制还支持精确恢复。 |
| `536eb8029`，损坏账号文件不被空表覆盖 | **核心保护已覆盖。** Claude 的 `persist_store` 先 `load_store_checked`；桌面 Agy manager 以 `load_failed` 禁止 persist。上游改为先 rename 留存再从空表继续，属于不同恢复体验；上游 rename 失败仍返回默认值，不能替换掉本地禁止写入的保护。CodeBuddy/WorkBuddy 部分不适用。 |
| `fe99cae4e`，#9773 的请求体/解压限额 | **已覆盖主要请求限制。** 当前所有保留的主要代理入口使用 `read_request_body_limited`，解压也受相同 200 MiB 预算约束，并有 413/压缩炸弹测试。其他 UI/资源释放小项不能仅由此认定全部等价，按实际需要另查。 |
| `43a825fc9`，#9754，Hyper 首包超时 | **已覆盖。** 当前 `forwarder.rs:2753` 计算统一 `header_timeout`，reqwest 和 hyper 均使用该值，不需要再合入旧路径重构。 |
| `dce5608be` 的防重定向、响应体限额、模板转义部分 | **已有实现。** 当前脚本使用 no-redirect client、2 MiB 响应上限，以及单遍 UTF-16 转义替换。只补第 2 节指出的剩余差异。 |
| `d2bb4c8d4` 的 DB/SQL 权限、主要备份文件名检查 | **大部分已覆盖。** 当前数据库及 sidecar 权限收紧、SQL 私有写入、路径/冒号/NUL 校验已存在。可顺手补 `.db` 空主名边界；重点仍是 CONNECT 部分。 |
| `9f1c1bb54`，#9833，非 loopback listener 的 Host 防护 | **已有不同的保护体系，不整体移植。** 当前 `browser_guard.rs:120、130` 结合 `PeerAddr`、loopback Host 校验与 LAN token；非 loopback 来访不能照搬上游“按连接地址放宽”的规则。若补地址识别，应先证明保持当前 token 策略与有意允许的认证客户端 Host 行为。 |
| `fd7e7697f`，#9541，SQL 导入后同步警告 | **已覆盖。** 当前 [useImportExport.ts](../src/hooks/useImportExport.ts) 读取 `result.warning`，合并界面刷新失败，显示 `partial-success`。 |
| `49e82664a`，#9593，编辑器异步保存串会话 | **已覆盖关键隔离。** 当前 `EditProviderDialog.tsx:47` 按 open/app/provider 建立独立 session，关闭动作检查 mounted；保留现有 live-edit 冲突重试，不用旧版弹窗实现替换。 |
| `30958c041`，#9844，深链接配置合并失败 | **阻止误导入已覆盖，呈现方式不同。** 当前失败后仍展示带 `previewError` 的确认框，但导入按钮禁用，`handleImport` 同样提前返回。上游选择直接不打开弹窗；这是体验选择，不应误判为当前可导入不完整配置。 |
| `18744a32d`，DSH reasoningEfforts 键规范化 | **核心读写行为已覆盖。** 当前 `dsh_config.rs` 有 `normalize_reasoning_efforts`、读取/写出调用，以及 false→off、非法键过滤的回归用例。无需重复移植核心补丁。 |
| `f29d0804b`；`99f4bae58` 的卡片查询隔离目标 | **二轮下调为可选整理。** 当前没有旧 Agy 查询路径；Codex 账号虽然由共享 query 推导，但显示分支由 `isCodexOauth`/`codexOfficialIdentity` 保护，额度 footer 也按类型挂载。没有据此确认其他卡片会显示 Codex 缓存账号；拆分 hook 可另做维护优化，不能作为已确认缺陷要求合并。 |
| `52f78ba1d`，#9823，元数据搜索懒扫描 | **已采用同类优化。** 当前 [useSessionSearch.ts](../src/hooks/useSessionSearch.ts) 在首次非空搜索时构建标准化文本数组，没有旧式全文子串索引。 |
| `3b7a7ed9b`，#9918，locale/tray 懒加载与隐藏暂停 | **托盘部分已覆盖，语言分包另作小范围评估。** 收尾时当前已恢复中文/英文，两份 locale 均静态导入；可按包体积再判断是否只加载当前语言。托盘 UsageTab 已 lazy import，查询由 `visible` 控制，隐藏时关闭轮询。不要为了源提交的多语言分包重新引入其余已移除语言。 |
| `5adff00c1` 的后端版本并发、`aaf6ee963` 的辅助 HTTP client 懒建 | **已有更符合当前范围的实现。** 保留 `buffered(4)` 和按需建立客户端；UI 逐项显示、读配置快照问题另行处理。 |
| `a18d91c56`；`2e2f3bbfa`、`92f61db17` 的主要会话行为 | 当前已有官方 Codex 模型 reasoning override、Agy 结构化会话及 T3/ACP 发现相关实现。无需为了相同目标再整体搬入上游文件；可把源回归用例用于后续差异复核。 |

## 6. 当前不建议合并的方向

| 提交或类别 | 不合并/暂缓原因 |
| --- | --- |
| `9431a6bc9` Copilot 账号迁移；`6da092fa2` Fusion Auto；`c18f4b0a9` Free Auto；`0d7f6e37e` CLI 配置联动及其他 Copilot 专属提交 | **不合并。** Copilot 已不支持，残留认证或路由代码不构成继续维护、恢复功能的理由。 |
| `0e8a584b6` xAI 托盘额度及其他 xAI 专属提交 | **不合并。** xAI 已不支持，不新增账号缓存、额度查询、托盘来源或专用路由。 |
| `37e3eabfe`，#9648，按真实 host 识别厂商 | **撤回原合并建议。** 本报告此前据以推荐的是 xAI 两处专用判断，已超出支持范围。该提交还含通用厂商识别改动，当前余额/套餐前端已有 URL 主机匹配；未单独证实其他受支持路径存在缺口前，不列为合并候选。 |
| `d26be6ecb` ZCode、`e6fd4accd` 及后续 CodeBuddy 功能 | 增加客户端，偏离当前五应用范围；也会带入账号、枚举、UI、预设与测试的大量改动。 |
| `d07f86ef3` Pi、`d187cc0e3` OpenClaw、`bb1b58a51` Gemini CLI、`1a3d75f1f` Hermes 等专属修改 | 对应独立客户端已移除。**Gemini 上游协议转换仍在，`1963028ee` 和 `8162adb3c` 都适用；混合提交 `3cf5865ca` 的通用工具发现部分也适用。** |
| `f985a04db`、`005f893d8`、插件功能链 | 当前没有 `src/components/plugins` 页面，不能为错误提示/行 key 修复恢复整套 Plugins 产品功能。混合 tools/plugin 提交只能独立提取适用于现有工具链的部分。 |
| `2372b1134`、`3da2c3cab`、`908a83eda`，全文会话搜索取消/索引/错误提示 | 当前没有上游 `session_manager/search.rs` 和全文搜索 IPC，只有元数据搜索及会话正文读取。属于恢复一个新功能栈，不能算现有性能补丁。 |
| `32773e88c`，#9826，privacy filter 优化 | 当前代理目录没有上游 privacy_filter 模块。无现有执行路径可优化，不为此恢复模块。 |
| `66817dc25` + `c8903de77`，官方 Fast 开关与文案 | 当前 `CodexFormFields.tsx` 已没有上游 `officialSpeedTiers` 开关。不能为了修一个已移除的控件恢复产品设置。 |
| `ba108cf1a`，#9978，Agy pending binding 云快照隔离 | 当前核心 `provider/agy_direct.rs` 是终端启动参数路径，没有上游 `save_from_editor` 里“写绑定→切登录→回滚”的流程。应先定位当前是否有等价的跨 DB/钥匙串事务，再决定移植锁；不能直接添加上游锁顺序。 |
| `99f4bae58`，#9975，Agy auth 缓存串卡片 | 当前 ProviderCard 不再直接查询该 Agy auth 状态，不恢复旧 hook。二轮核对 Codex 显示分支后，也未确认同类串号；参见第 5 节。 |
| `7fe930858`，#10006，应用筛选顺序 | 当前已是五应用固定列表，上游主要调整已移除应用的位置。若未来引入用户自定义排序，应从当前 sidebar 顺序派生，不复制上游长名单。 |
| 大批测试搬家、删除测试、删除导出、forwarder 拆文件等 refactor/chore | 当前目录与职责已独立演进。可以学习结构，但没有必要为后续一个小修复先导入整段重构历史；优先手工移植功能和所需测试。 |
| `d9ebf421d`，#9872，CI 加速 | 当前没有上游同结构 CI 工作流，核心/桌面/CLI 也已分包；应基于本仓库测试入口设计，不直接复制上游矩阵与链接目标设置。 |
| 发布版本、赞助商预设、README/多语言批量同步 | 与当前独立 1.0.0 发布及中英双语范围不同，不进入修复合并批次。 |

混合提交按受支持路径拆分判断：只有通用修复在当前支持范围内有独立证据，才值得提取；已退出支持范围的服务不因共用 Claude/Codex 入口而重新成为候选。

同理，Agy 保存绑定同步 `358da7a38` 是 `ba108cf1a` 所保护的流程来源。当前修改绑定是否立即改变全局登录，应先明确产品行为；若以后引入此流程，应一起设计同步/回滚与云快照隔离，不能只加入其中一把锁。

## 7. 建议实施顺序与验收重点

本节是后续实施方案，本次没有执行。

| 批次 | 内容 | 合并前必须得到的证据 |
| --- | --- | --- |
| 1 | P0：`e8eb86d30`、`b1d7598d7`、`31575905b`；随后补 `d2bb4c8d4`、`dce5608be`、`bf4cc69fe` 的缺口 | 人工可控的旧请求完成顺序；DB 与 UI 路由都不回退；SQL/WebDAV/二进制恢复拒绝禁止对象且不改真实数据；MCP 实际写出和预览一致；代理切换读快照一致；桌面/CLI 同时通过 |
| 2 | `d15cb087c`、`82c29d09a`、`148c1005b`、`1963028ee` | 流式/非流式转换一致；普通模型 schema 不被 Gemini 分支改变；转换结果历史回放不出现 ID/工具映射回归 |
| 3 | `f2b3c12eb` + `84e4612c7`、`0c1121ce3`、`c73d8a48c`、`1fba4e16c`、`46726137c`、`8162adb3c` | 长流头尾 usage 不丢；多事件大 chunk 与单事件超限区分；UTF-8/CRLF 边界；Gemini 超过 64 轮仍按精确调用 ID 回放且缓存有界；并行工具、重试、断流、取消和不同 provider 分支 |
| 4 | `4c9e5c3b0`、`3c49b5cdc` 联查 `d910a92d8`、Skills/Unix 工具链修复含 `3cf5865ca`、`51657ffae` | OpenCode V1/V2/2.4 fixtures；正文、工具块、分页读取和删除一致；升级/卸载计划指向实际包；Unix launcher 身份及可执行权限；慢传输可完成、停滞能超时；系统终端参数正确 |
| 5 | `d4290287b`、`9967927cb`、`c131f68e9`，再做 `3b08b1162`/`aaf6ee963`、`e7915ecfa`、`23b937b20` | 先固定统计口径再优化；相同数据库/会话 fixture 结果一致；不双计费；记录耗时、线程、内存与锁等待，不能只报告“代码更少” |
| 6 | `83a3161fc`、`4e10bd857`、`fcff029cf` 及条件候选；体验项 `2a0f43331` 可独立排期 | 性能项独立测量与故障场景验证；配置缓存包括 CLI 外部写入失效；macOS 菜单栏退出和更新重启由对应平台验收 |

### 7.1 应按行为携带的依赖

| 修复组 | 应一起提取或保留的内容 |
| --- | --- |
| 路由与代理 | `e8eb86d30` 的请求版本必须同时约束 `current_providers` 和最终 `record_failover_route` 写入；只在异步 hook 中判断仍会留下状态回退。`bf4cc69fe` 的快照要覆盖传输路径选择和所取 Client，保留本地设置失败补偿。 |
| MCP/CONNECT | `b1d7598d7` 同时涉及导入边界、`McpConfirmation`、`deeplinkRisk.ts` 与中英提示；后者还应核对 `d2bb4c8d4` 的新增规则。不要把 UI 风险提示当成禁止保存的校验。 |
| SSE/协议 | `f2b3c12eb` 与 `84e4612c7` 按头尾保留及逐事件限额一起适配。`8162adb3c` 同时改 shadow store 与转换器，并影响本地 Codex→Gemini 桥接，不能只增大完整 turn 数量。 |
| OpenCode 工具 | `3c49b5cdc` 覆盖发现、版本、安装/升级/卸载命令；`d910a92d8` 的 Desktop 来源保护是相关前置行为核对，`3cf5865ca` 影响 Unix 启动入口身份。缺少某个旧提交 SHA 不代表必须先整笔 cherry-pick。 |
| 统计性能 | 先固定 `d4290287b` 等统计口径，再取 `3b08b1162` 的一次扫描/中位数选择，最后取 `aaf6ee963` 的分配优化。测量前不能把“一次扫描”写成具体加速倍数。 |

Rust 验证应按当前 crate 分别跑核心、桌面适配和 CLI 的相关测试；不能使用上游单体 crate 的通过结果替代。涉及前端时运行对应 Vitest 用例和 `pnpm typecheck`，涉及启动/分包时再检查 renderer build。仅修改文档的本次没有运行这些代码测试。

当前工作区已经有大量未提交代码，实施时应以用户当时的完整工作区为基线建立可回退快照，不能从现有 HEAD 新建空白分支后把本报告当作适用结论，也不能 stash/reset 掉用户修改来“方便 cherry-pick”。

## 8. 复核入口与限制

重点证据文件：

- 路由与代理：[forwarder.rs](../crates/cc-switch-core/src/proxy/forwarder.rs)、[controller.rs](../crates/cc-switch-core/src/mode/controller.rs)、[http_client.rs](../crates/cc-switch-core/src/proxy/http_client.rs)、[global_proxy.rs](../src-tauri/src/commands/global_proxy.rs)。
- 导入与数据保护：[MCP 导入](../src-tauri/src/deeplink/mcp.rs)、[MCP 确认](../src/components/deeplink/McpConfirmation.tsx)、[数据库备份/导入](../crates/cc-switch-core/src/database/backup.rs)、[用量脚本](../crates/cc-switch-core/src/usage_script.rs)。
- 二轮导入调用链：[MCP service](../crates/cc-switch-core/src/services/mcp.rs)、[MCP 基础校验](../crates/cc-switch-core/src/mcp/validation.rs)、[Claude MCP 写出](../crates/cc-switch-core/src/mcp/claude.rs)、[深链接风险提示](../src/utils/deeplinkRisk.ts)。
- 二轮协议与显示：[Gemini shadow](../crates/cc-switch-core/src/proxy/providers/gemini_shadow.rs)、[Gemini 转换](../crates/cc-switch-core/src/proxy/providers/transform_gemini.rs)、[ProviderCard 显示分支](../src/components/providers/ProviderCard.tsx)。
- 二轮工具与托盘：[工具发现](../src-tauri/src/commands/tool_discovery.rs)、[工具升级计划](../src-tauri/src/commands/tool_lifecycle_command.rs)、[终端启动](../src-tauri/src/commands/misc.rs)、[托盘](../src-tauri/src/tray.rs)。
- 流式与统计：[SSE buffer](../crates/cc-switch-core/src/proxy/sse.rs)、[response_processor.rs](../crates/cc-switch-core/src/proxy/response_processor.rs)、[database/usage.rs](../crates/cc-switch-core/src/database/usage.rs)。
- 会话与工具：[OpenCode reader](../crates/cc-switch-core/src/session_manager/providers/opencode.rs)、[Claude 会话统计](../crates/cc-switch-core/src/services/session_usage.rs)、[Codex 会话统计](../crates/cc-switch-core/src/services/session_usage_codex.rs)、[工具版本](../src-tauri/src/commands/tool_version.rs)。
- 已有独立实现：[环境快照](../src-tauri/src/services/env_manager/snapshot.rs)、[Claude 账号存储](../crates/cc-switch-core/src/services/claude_account/mod.rs)、[Agy 账号存储](../src-tauri/src/services/agy_account/mod.rs)、[本地性能工作记录](performance-2026-11-27.md)。

可用以下只读命令复核源提交；短 SHA 均属于源仓库：

```powershell
git -C D:\tmp\cc-switch\cc-switch-source log 09acca049..078ea4a85 --format="%h %cs %s"
git -C D:\tmp\cc-switch\cc-switch-source show --stat e8eb86d30
git -C D:\tmp\cc-switch\cc-switch-source show e8eb86d30 -- src-tauri/src/proxy/forwarder.rs
git -C D:\tmp\cc-switch\cc-switch-source show 4c9e5c3b0 -- src-tauri/src/session_manager/providers/opencode.rs
```

报告中的行号对应分析时工作区，仅帮助定位。没有执行 cherry-pick/apply 的冲突试探，成本判断来自结构和调用链差异；不能把“低成本”理解为保证无冲突。真实上游服务、平台行为和性能收益均需在相应实施批次验证。

二轮未把未列出的所有 fix 自动归为“不适用”或“已经覆盖”；这是有证据的合并候选清单，不是全量缺陷审计。尤其是平台重启、打包和其他交互改动，仍可能值得在对应发布目标确定后另行验证。
