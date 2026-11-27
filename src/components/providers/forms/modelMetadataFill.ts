// 从拉取列表选中模型后，把查到的已知参数换成各应用的字段格式补进这一行。
// 统一规则：只补空着的字段，用户填过的值一律不动；布尔和模态只往「支持」方向补。
import type { KnownModelMetadata } from "@/lib/modelMetadata";
import type { CodexCatalogModel, OpenCodeModel } from "@/types";
/** 补全前后是否不同：没补上任何字段时不提示用户。 */
export const metadataFilledAnything = (before: unknown, after: unknown) =>
  JSON.stringify(before) !== JSON.stringify(after);
const isBlank = (value: unknown) =>
  value === undefined || value === null || String(value).trim() === "";
/** 只认文字和图片两种输入（Codex / OpenClaw / Pi 的模态字段只有这两种）。 */
function textImageModalities(
  modalities: string[] | undefined,
): string[] | undefined {
  if (!modalities) return undefined;
  return modalities.includes("image") ? ["text", "image"] : ["text"];
}
export function fillCodexCatalogModel<T extends CodexCatalogModel>(
  row: T,
  metadata: KnownModelMetadata,
  knownLevels: readonly string[],
): T {
  const next = { ...row };
  if (isBlank(row.contextWindow) && metadata.contextWindow) {
    next.contextWindow = String(metadata.contextWindow);
  }
  if (!row.reasoningLevels?.length && metadata.reasoningEfforts) {
    // 按 Codex 的档位顺序排，丢掉它不认识的值。
    const levels = knownLevels.filter((level) =>
      metadata.reasoningEfforts?.includes(level),
    );
    if (levels.length > 0) next.reasoningLevels = levels;
  }
  // 只补「支持图片」：没写时后端按注册表判断、认不出就当支持；查到只收文字就写
  // ["text"] 会关掉 Codex 的图片入口，而编辑界面又没有改回来的地方（#8682）。
  if (!row.inputModalities && metadata.inputModalities?.includes("image")) {
    next.inputModalities = textImageModalities(metadata.inputModalities);
  }
  return next;
}
export function fillOpenCodeModel(
  model: OpenCodeModel,
  metadata: KnownModelMetadata,
): OpenCodeModel {
  const next = { ...model };
  const limit = { ...(model.limit ?? {}) };
  if (isBlank(limit.context) && metadata.contextWindow) {
    limit.context = metadata.contextWindow;
  }
  if (isBlank(limit.output) && metadata.maxOutputTokens) {
    limit.output = metadata.maxOutputTokens;
  }
  if (Object.keys(limit).length > 0) next.limit = limit;
  if (model.modalities === undefined && metadata.inputModalities) {
    next.modalities = {
      input: metadata.inputModalities,
      output: metadata.outputModalities ?? ["text"],
    };
  }
  // OpenCode 只给 reasoning 为真的模型生成思考档位（ctrl+t）。只补 true：
  // 写 false 会盖住 models.dev 同名条目，用户明确写的 false 也不动。
  if (model.reasoning === undefined && metadata.reasoning === true) {
    next.reasoning = true;
  }
  return next;
}
