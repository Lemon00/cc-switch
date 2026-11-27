# 测试与验证

以下命令在仓库根目录执行，依据当前 `package.json`、各 crate 的 `Cargo.toml` 和测试目录核对。按修改范围选择测试；文档中的命令不构成当前工作树已全部通过的声明。

## 前端

```bash
pnpm install --frozen-lockfile
pnpm test:unit
pnpm vitest run tests/components/DeepLinkImportDialog.test.tsx
pnpm typecheck
```

[vitest.config.ts](../vitest.config.ts) 收集 `src/` 与 `tests/` 中的 `*.test.*` 和 `*.spec.*`，默认使用 jsdom。[setupGlobals.ts](../tests/setupGlobals.ts) 和 [setupTests.ts](../tests/setupTests.ts) 准备环境；`tests/msw/tauriMocks.ts` 将 Tauri 调用接到测试用传输，MSW handlers 提供响应。组件测试并不启动真实 WebView。

异步读取的回归应覆盖迟到结果、应用切换、卸载、失败后重试和写入后刷新；使用真实 QueryClient 的测试可验证缓存失效与取消行为，而不只断言 mock 被调用。

## Rust 单元测试

核心库、桌面层与 CLI 分开测试。核心库单独测试会生成本地 `Cargo.lock`，该锁文件按仓库规则不提交；GUI/CLI 使用各自已提交的锁文件。

先设置隔离配置目录。PowerShell：

```powershell
$testHome = Join-Path (Get-Location) ('target/test-home-' + [guid]::NewGuid().ToString('N'))
New-Item -ItemType Directory -Force -Path $testHome | Out-Null
$env:CC_SWITCH_TEST_HOME = $testHome
```

macOS / Linux：

```bash
export CC_SWITCH_TEST_HOME="$(mktemp -d)"
```

然后分别运行：

```bash
cargo test --manifest-path crates/cc-switch-core/Cargo.toml --lib -- --test-threads=1
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- --test-threads=1
cargo test --locked --manifest-path crates/cc-switch-cli/Cargo.toml -- --test-threads=1
```

CLI 常规测试包含 [outbound_proxy.rs](../crates/cc-switch-cli/tests/outbound_proxy.rs)：它使用 Cargo 构建的真实 CLI 和隔离配置启动后台代理，确认数据库保存的出站 HTTP 代理实际接收到请求，并覆盖合法但不可达地址保留、无效地址清理和停止后的 PID/端口释放。仅测试核心初始化函数或本机上游直连，不能替代这项宿主入口回归。单独运行：

```bash
cargo test --locked --manifest-path crates/cc-switch-cli/Cargo.toml --test outbound_proxy -- --test-threads=1
```

首次编译桌面 crate 前需准备 `dist/`：可运行 `pnpm build:renderer`，纯后端测试也可仅创建空目录（PowerShell：`New-Item -ItemType Directory -Force dist`；POSIX：`mkdir -p dist`）。空目录不能代替实际前端构建或桌面验收。

涉及进程环境、全局设置缓存或共享目录的 Rust 用例使用串行执行。测试夹具必须创建自身依赖的配置、应用安装目录并恢复环境；例如深链接夹具显式写入默认设置，Antigravity MCP 落盘用例先创建隔离安装目录。缺失文件应留给专门的失败场景测试，不应靠真实用户配置补齐。不要为运行测试修改 shell 的 `HOME` 或 `USERPROFILE`；需要改变这些变量的用例由自身 guard 管理。

GNU 宿主工具链和 DLL/清单问题见 [Windows GNU 构建](windows-gnu-build.md)。共享 `CARGO_TARGET_DIR` 时 Cargo 会等待其他任务的构建锁；不要并发启动重复编译，也不要删除正在使用的锁文件。

## 模块 13：托盘、深链接与平台集成

前端回归：

```bash
pnpm vitest run tests/components/DeepLinkImportDialog.test.tsx tests/tray-panel tests/components/TrayPanel.test.tsx tests/components/TrayPanelUsageTab.test.tsx tests/components/TrayPanel.styles.test.ts tests/utils/deepLinkConfigPreview.test.ts tests/components/GeneralSection.trayStyle.test.tsx tests/hooks/useTrayNavigation.test.tsx
```

Rust 回归（多个过滤器按“或”匹配）：

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml --lib -- deeplink:: tray:: tray_panel:: window_fit:: auto_launch:: --test-threads=1
```

| 测试入口                                                                                                                                   | 主要契约                                                   |
| ------------------------------------------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------- |
| [DeepLinkImportDialog.test.tsx](../tests/components/DeepLinkImportDialog.test.tsx)                                                         | 配置预览、单次提交、刷新失败、旧导入完成时保留新链接确认框 |
| [usePanelState.test.tsx](../tests/tray-panel/usePanelState.test.tsx)                                                                       | 通知合并、过期响应、失败保留快照和卸载                     |
| [TrayPanel.test.tsx](../tests/components/TrayPanel.test.tsx)、[TrayPanelUsageTab.test.tsx](../tests/components/TrayPanelUsageTab.test.tsx) | 显示序号、隐藏行为、用量缓存及错误重试                     |
| [parser.rs](../src-tauri/src/deeplink/parser.rs)、[deeplink/tests.rs](../src-tauri/src/deeplink/tests.rs)                                  | 输入预算、重复参数、类型、URL 校验、导入语义               |
| [provider.rs](../src-tauri/src/deeplink/provider.rs)                                                                                       | 配置合并、供应商 ID 唯一性                                 |
| [tray.rs](../src-tauri/src/tray.rs)、[tray_panel.rs](../src-tauri/src/tray_panel.rs)                                                       | 菜单数据、查询间隔溢出与面板几何                           |
| [window_fit.rs](../src-tauri/src/window_fit.rs)、[auto_launch.rs](../src-tauri/src/auto_launch.rs)                                         | 工作区和缩放边界、AppImage 启动路径选择                    |

这些用例覆盖可自动验证的逻辑；真实窗口线程、焦点、系统托盘、自启注册及平台装饰仍需对应平台的桌面测试。

## 集成、编译与发布检查

需要验证真实数据库和文件调用链时，单独选择集成目标，例如：

```bash
cargo test --locked --manifest-path src-tauri/Cargo.toml --test deeplink_import -- --test-threads=1
```

`src-tauri/tests/support.rs` 提供共享目录隔离；`src-tauri/tests/golden/` 的快照是测试输入/预期结果，不是可删除的闲置文档。

```bash
pnpm format:check
pnpm build:renderer
cargo fmt --manifest-path crates/cc-switch-core/Cargo.toml --check
cargo fmt --manifest-path src-tauri/Cargo.toml --check
cargo fmt --manifest-path crates/cc-switch-cli/Cargo.toml --check
cargo check --locked --manifest-path src-tauri/Cargo.toml --tests
cargo build --locked --manifest-path crates/cc-switch-cli/Cargo.toml
```

`pnpm format:check` 只覆盖 `src/`，不包含文档和 `tests/`。单元测试、类型检查、编译、前端产物、安装包及桌面实测是不同验证项，报告时分别说明结果。

当前仓库的 [release.yml](../.github/workflows/release.yml) 在 tag 推送时检查更新摘要与发布元数据，再构建 Windows/Linux 产物；它不是全量测试矩阵，也没有 macOS 测试任务。当前版本摘要、发布正文缺失，版本清单或 tag 不一致，以及摘要结构不合法都会使校验失败。

## 独立版本发布回归

```bash
pnpm vitest run tests/config/releaseVersion.test.ts tests/config/whatsNewEntries.test.ts tests/lib/whatsNew.test.ts tests/components/WhatsNewNotice.test.tsx tests/components/NewLayoutDialog.test.tsx tests/lib/updater.test.ts tests/contexts/UpdateContext.test.tsx
cargo test --manifest-path crates/cc-switch-core/Cargo.toml --test independent_release_settings
```

前端验证版本清单与本地锁文件一致、发布资料存在，以及更新提示和链接；Rust 目标单独验证设置序列化，确保旧版 `whatsNewSeenVersion` 与独立版 `independentWhatsNewSeenVersion` 分开保存。它不启动桌面、不读写真实配置，也不代表核心库的全部内部测试已运行。
