# Windows GNU 构建与 Cargo 新目录兼容性

## 修复背景

原锁文件的 `tauri-build 2.5.6` 假设 `OUT_DIR` 始终是：

```text
<profile>/build/<package>-<hash>/out
```

本机 Cargo 1.100.0 实际使用：

```text
<profile>/build/<package>/<hash>/out
```

旧实现固定向上取三层目录，再扫描 `build` 以查找 GNU 的 `WebView2Loader.dll`，因此扫描到不存在的 `build/build` 并报 `os error 3`。

本次采用上游正式修复（Tauri PR #16164），不修改 Cargo 下载缓存，不创建假目录，也不通过关闭检查绕过问题。新版根据祖先中的 `build` 目录识别输出根目录，并分别处理新旧布局下的 WebView2Loader 查找。

## 依赖对齐

| 依赖 | 修复前锁定值 | 本次锁定值 |
| --- | --- | --- |
| `tauri-build` | 2.5.6 | 2.7.6 |
| `tauri` | 2.10.3 | 2.12.7 |
| `tauri-utils` | 2.8.3 | 2.10.4 |
| `tauri-codegen` / `tauri-macros` | 2.5.5 | 2.7.6 |
| `tauri-runtime` | 2.10.1 | 2.12.4 |
| `tauri-runtime-wry` | 2.10.1 | 2.12.5 |
| `webview2-com` | 0.38.2 | 0.39.1 |
| GUI 直接依赖 `windows` | 0.61.3 | 0.62.2 |

只升级 `tauri-build` 曾实测触发旧版 `tauri::test` 初始化 `AppConfig` 时缺少 `app_directories_override` 的编译错误，因此同步对齐 Tauri 主库、代码生成和 Windows COM 接口依赖。插件版本未主动升级；锁文件同时更新上述包必需的传递依赖，不是对所有 Rust 依赖执行全量更新。

新 Tauri 包声明最低 Rust 1.90，GUI crate 的 `rust-version` 相应改为 `1.90.0`。仓库固定工具链 `1.95` 保持不变，也不改变本机全局默认工具链。

原有 Windows 清单逻辑保持：通过独立 RC 文件给程序及测试嵌入 Common Controls v6，不输出 MSVC 专用 `/MANIFEST` 参数。

## 本机复跑

在仓库根目录的 PowerShell 中运行。GNU 工具目录按实际安装位置修改；所有变量仅影响当前进程。

```powershell
$gnuBin = 'D:\msys64\ucrt64\bin'
$env:PATH = "$gnuBin;" + $env:PATH
$env:CC = Join-Path $gnuBin 'gcc.exe'
$env:CXX = Join-Path $gnuBin 'g++.exe'
$env:AR = Join-Path $gnuBin 'ar.exe'
$env:RC = Join-Path $gnuBin 'windres.exe'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $env:CC
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target\windows-gnu'
$env:CC_SWITCH_TEST_HOME = Join-Path (Get-Location) 'target\windows-gnu-test-home'

cargo +stable-x86_64-pc-windows-gnu check --locked --manifest-path src-tauri/Cargo.toml --tests --jobs 4
cargo +stable-x86_64-pc-windows-gnu test --locked --manifest-path src-tauri/Cargo.toml --test windows_manifest --jobs 4 -- --test-threads=1
```

必须选择 GNU **宿主工具链**，而不只是给默认 MSVC 工具链增加 `--target`。后者仍可能让构建脚本和过程宏尝试调用 MSVC 的 `link.exe`。GNU 构建仍需要 GCC/GNU 链接器；不是完全不需要链接器。

首次需要下载锁文件中的依赖。缓存齐备后可增加 `--offline`。`stable-x86_64-pc-windows-gnu` 是本机现有工具链名，本次版本为 Rust/Cargo 1.100.0；后续运行应记录实际 `rustc -V` / `cargo -V`，不要把移动的 stable 名称当作固定版本。

## 回归与验收范围

`src-tauri/tests/windows_manifest.rs` 包含：

- 从正在运行的测试进程读取嵌入资源，验证 Common Controls v6 清单。
- GNU 下根据 Cargo 给出的应用程序路径，检查同目录存在有效 PE 开头的 `WebView2Loader.dll`，避免构建仅绕过错误却遗漏 DLL。

这两项测试不启动 GUI，不修改用户注册表、客户端配置或账户。

2026-11-23 实测结果：

- `cargo +stable-x86_64-pc-windows-gnu check --locked --manifest-path src-tauri/Cargo.toml --tests --jobs 4` **通过**，退出码 0。已覆盖 GUI 库、程序测试目标和集成测试的编译检查，不再停在构建脚本阶段。
- 使用上述真实 GUI 构建生成的 `libcommon-controls.a`，独立编译运行同一个 `windows_manifest.rs`：**2 项全部通过**。测试检查的 DLL 就是上述 Tauri 构建复制到目标目录的文件，没有另行伪造或手动复制 DLL。
- 资源测试采用 `rustc --test` 独立链接，手动提供应用目标路径用于定位 DLL；本轮没有执行前面列出的完整 `cargo test --test windows_manifest` 命令，也没有链接完整 GUI 应用。
- `rustfmt --check` 和 `git diff --check` **通过**。
- 对已缓存锁定包的 registry 元数据检查未发现声明 Rust 最低版本高于仓库固定 1.95 的包。这不是实际的 1.95 编译验证。
- 编译仍有既有 unused/dead-code 告警及 Rust 1.100 的弃用告警，本轮没有进行无关清理。

`cargo check` 只代表编译检查，不等价于 GUI 启动、完整业务测试或安装包验收。Linux/macOS 和 MSVC 仍需各自的 CI/实机验证。
