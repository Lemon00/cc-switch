# 更新摘要（应用内弹窗）

用户升级后第一次打开 CC Switch，会弹一个窗口，列出从上次看过的版本到当前版本之间每一版的几条摘要。数据就是这个目录下的文件，构建时打进安装包（`src/lib/whatsNew.ts`），弹窗不联网。

详细内容放在 `docs/release-notes/v<版本>-zh.md`。发布工作流直接读取该文件作为 GitHub Release 正文，弹窗「详情」打开 `Lemon00/cc-switch` 对应 tag 的 Release；不再同步上游官网。

独立版本的已读记录使用 `settings.json` 的 `independentWhatsNewSeenVersion`，旧 `whatsNewSeenVersion` 不参与比较。因此旧版 4.x 记录不会阻止 1.0.0 的提示；独立版本内部降级仍保留较高的已读版本。

## 每个版本一个文件

文件名是版本号，例如 `1.0.0.json`：

```json
{
  "version": "1.0.0",
  "items": [
    {
      "type": "new",
      "zh": "OpenCode 可以选择思考档位",
      "en": "Choose a thinking level for OpenCode"
    }
  ]
}
```

- `type`：`new`（新增）、`fix`（修复）、`improve`（改进）。
- 每版最多 4 条，每条提供简体中文和英文；中文不超过 40 字，英文不超过 100 个字符。
- 写用户能感知到的变化，按影响大小排序。从中文发布说明里挑最重要的几条，不要照搬原文。
- `"items": []` 表示这一版不弹窗，适合纯构建、CI 之类的版本。文件仍然要有。

规则由 `tests/config/whatsNewEntries.test.ts` 检查。

## 什么时候写

与发布说明一起，在打 tag 前提交。`release.yml` 要求当前版本摘要和发布正文存在，运行 `tests/config/whatsNewEntries.test.ts` 与 `tests/config/releaseVersion.test.ts`，校验摘要格式、版本清单、锁文件及 tag。失败会阻断发布。发布矩阵和验证范围见[测试指南](../../docs/testing.md)。
