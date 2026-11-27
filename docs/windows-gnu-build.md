# Windows GNU 构建

GNU 构建用于没有 MSVC 开发环境的本地开发；发布工作流的具体工具链以 `.github/workflows/release.yml` 为准。依赖版本由各 crate 的锁文件决定，不手工修改 Cargo 下载缓存。

## 环境

安装与仓库 `rust-toolchain.toml` 对应版本的 GNU **宿主工具链**，以及提供 GCC、G++、ar 和 windres 的 MinGW 工具集。只给 MSVC 宿主添加 GNU target，构建脚本和过程宏仍可能调用 MSVC `link.exe`。

在仓库根目录的 PowerShell 中设置，路径按本机调整：

```powershell
$gnuBin = 'C:\msys64\ucrt64\bin'
$env:PATH = "$gnuBin;" + $env:PATH
$env:CC = Join-Path $gnuBin 'gcc.exe'
$env:CXX = Join-Path $gnuBin 'g++.exe'
$env:AR = Join-Path $gnuBin 'ar.exe'
$env:RC = Join-Path $gnuBin 'windres.exe'
$env:CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER = $env:CC
$env:CARGO_TARGET_DIR = Join-Path (Get-Location) 'target/windows-gnu'
$env:CC_SWITCH_TEST_HOME = Join-Path (Get-Location) 'target/windows-gnu-test-home'
New-Item -ItemType Directory -Force $env:CC_SWITCH_TEST_HOME | Out-Null
```

先准备前端资源，再使用已安装的 GNU 宿主工具链执行：

```powershell
pnpm build:renderer
cargo +1.95-x86_64-pc-windows-gnu check --locked --manifest-path src-tauri/Cargo.toml --tests
cargo +1.95-x86_64-pc-windows-gnu test --locked --manifest-path src-tauri/Cargo.toml --test windows_manifest -- --test-threads=1
```

命令中的版本应与 `rust-toolchain.toml` 保持一致。若使用其他已安装版本，记录实际 `rustc -V` / `cargo -V`；移动的 `stable` 名称不是固定版本。首次需下载依赖，缓存齐备后可以加 `--offline`。仅运行后端单元测试时的 `dist/` 准备方法见[测试指南](testing.md)。

## 常见问题

- **找不到链接器或资源编译器**：核对上述可执行文件、PATH 与工具链宿主，不要混用 MSVC 链接参数。
- **找不到 WebView2Loader.dll**：确认使用仓库锁文件及配套 Tauri 依赖；不要创建假的 `build/build` 目录绕过构建失败。新的 Cargo 输出布局需要相应版本的 Tauri 构建支持。
- **临时目录不可写**：为当前 PowerShell 进程设置可写的 `TEMP`、`TMP`，重新运行失败命令，不改动系统级配置。
- **等待构建锁**：检查是否已有 Cargo 任务使用相同目标目录，避免重复构建。

## 验证边界

[windows_manifest.rs](../src-tauri/tests/windows_manifest.rs) 检查进程嵌入的 Common Controls v6 清单，以及 GNU 应用目标附近的 WebView2Loader DLL。单元测试、`cargo check`、完整链接和实际桌面启动是不同检查，分别报告结果。
