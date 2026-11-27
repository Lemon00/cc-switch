# 发布清单脚本

这些脚本供本仓库的发布流程和可选下载镜像使用，不依赖上游官网。使用仓库 `.node-version` 指定的 Node.js；脚本仅用 Node 内置模块。

## 下载清单

```bash
node scripts/generate-download-manifest.mjs release-assets v1.0.0 https://github.com/Lemon00/cc-switch/releases/download release-assets/manifest.json
```

参数依次为产物目录、tag、不含 tag 的下载地址前缀、可选输出路径、可选发布时间（ISO 8601）。不传输出路径时写入当前目录的 `manifest.json`；不传时间时使用生成时间（传空字符串会报错）。

清单保留 `version`、`tag`、`pubDate`、`files` 结构。每项文件包含 `platform`、`kind`、`arch`、`name`、`size`、`sha256`、`url`。当前发布流程的五种下载均纳入：Windows MSI、Windows 便携 ZIP、Linux DEB、Ubuntu GUI 压缩包，以及独立 `cc-switch-cli`。CLI 包含 `tui` 子命令。

- GUI 压缩包的 `kind` 为 `portable`；CLI 的 `kind` 为 `cli`，下载页面应为 CLI 提供独立入口。
- `cc-switch-cli` 按当前发布流程标记 Linux x64；手动提供的 `cc-switch-cli.exe` 标记 Windows x64。其他架构的 CLI 需要先新增明确的命名与分类规则。
- 只匹配传入 tag 的 `CC-Switch-<tag>-…` 文件；含 `/` 的 tag 在文件名中替换为 `-`，在 URL 中编码为单个路径段。
- 保留旧 macOS、ARM64、RPM、AppImage 分类以支持已有产物；这些规则不表示当前发布矩阵会构建对应版本。
- 忽略目录、签名、更新器专用压缩包、说明文档（含 `CLI-TUI-README.md`）和清单本身。没有匹配产物时失败，不覆盖已有输出。

发布工作流自动生成并上传 `manifest.json`、`CLI-TUI-README.md`；无 updater 签名时仍可生成下载清单。

## 更新器清单（`latest.json`）

```bash
node scripts/generate-updater-manifest.mjs release-assets v1.0.0 https://github.com/Lemon00/cc-switch/releases/download release-assets/latest.json
```

参数依次为产物目录、tag、不含 tag 的下载地址前缀、可选输出路径（默认 `latest.json`）、可选发布时间（ISO 8601，与下载清单共享同一时间戳）、可选发布备注（默认 `Release <tag>`）。

脚本按当前 tag 精确匹配更新器平台产物及其非空 `.sig` 文件（`windows-x86_64` 对应 `-Windows.msi`，`linux-x86_64-deb` 对应 `-Linux-x86_64.deb`），使用与下载清单一致的 URL 编码规则生成 `latest.json`。任一平台缺失安装包或签名、或输入非法时失败且不覆盖已有输出。

## 更新器镜像清单

```bash
node scripts/rewrite-updater-manifest.mjs release-assets/latest.json v1.0.0 https://downloads.example.com/cc-switch release-assets/latest-mirror.json
```

仅在配置镜像时使用。默认输出名仍为 `latest-r2.json`，兼容已有调用。脚本校验版本、tag、完整的 GitHub Release 产物 URL 和非空签名，只替换下载地址，保留签名、平台键和其他元数据。镜像需要先提供与原文件完全一致的产物。全部条目验证通过后才写输出；缺失签名时不要用下载清单代替 updater 清单。

三个脚本的地址前缀均要求 HTTP(S)，不接受凭据、查询参数或 fragment。

## 验证

```bash
pnpm test:scripts
```

测试通过真实子进程在临时目录生成清单，覆盖当前产物、CLI 单文件发布、SHA-256、URL 编码、更新器清单生成与镜像重写往返、签名保留以及无效输入时保留原输出。
