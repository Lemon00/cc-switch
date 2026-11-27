import type { Prompt } from "@/lib/api/prompts";
import type { SkillDiscoveryResult } from "@/lib/api/skills";
const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const optionalString = (value: unknown) =>
  value === undefined || typeof value === "string";
const optionalNumber = (value: unknown) =>
  value === undefined || (typeof value === "number" && Number.isFinite(value));
/** 持久化缓存可能来自旧版本或被手工修改，不能直接断言为 IPC DTO。 */
export function isPromptCache(value: unknown): value is Record<string, Prompt> {
  return (
    isRecord(value) &&
    Object.entries(value).every(
      ([id, prompt]) =>
        isRecord(prompt) &&
        prompt.id === id &&
        typeof prompt.name === "string" &&
        typeof prompt.content === "string" &&
        typeof prompt.enabled === "boolean" &&
        optionalString(prompt.description) &&
        optionalNumber(prompt.createdAt) &&
        optionalNumber(prompt.updatedAt),
    )
  );
}
export function isSkillDiscoveryCache(
  value: unknown,
): value is SkillDiscoveryResult {
  return (
    isRecord(value) &&
    Array.isArray(value.skills) &&
    Array.isArray(value.failures) &&
    value.skills.every(
      (skill) =>
        isRecord(skill) &&
        [
          "key",
          "name",
          "description",
          "directory",
          "repoOwner",
          "repoName",
          "repoBranch",
        ].every((key) => typeof skill[key] === "string") &&
        optionalString(skill.readmeUrl),
    ) &&
    value.failures.every(
      (failure) =>
        isRecord(failure) &&
        ["owner", "name", "branch", "error"].every(
          (key) => typeof failure[key] === "string",
        ),
    )
  );
}
