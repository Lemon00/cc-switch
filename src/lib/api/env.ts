import type { BackupInfo, EnvConflict } from "@/types/env";
import { extractErrorMessage } from "@/utils/errorUtils";
import { invoke } from "@tauri-apps/api/core";
/**
 * 环境变量管理 API
 */
/**
 * 检查指定应用的环境变量冲突
 * @param appType 应用类型 ("claude" | "codex")
 * @returns 环境变量冲突列表
 */
export async function checkEnvConflicts(
  appType: string,
): Promise<EnvConflict[]> {
  return invoke<EnvConflict[]>("check_env_conflicts", { app: appType });
}
/**
 * 删除指定的环境变量 (会自动备份)
 * @param conflicts 要删除的环境变量冲突列表
 * @returns 备份信息
 */
export async function deleteEnvVars(
  conflicts: EnvConflict[],
): Promise<BackupInfo> {
  return invoke<BackupInfo>("delete_env_vars", { conflicts });
}
/**
 * 从备份文件恢复环境变量
 * @param backupPath 备份文件路径
 */
export async function restoreEnvBackup(backupPath: string): Promise<void> {
  return invoke<void>("restore_env_backup", { backupPath });
}
export type EnvConflictCheckResult =
  | {
      ok: true;
      conflicts: EnvConflict[];
    }
  | {
      ok: false;
      error: string;
    };
function envCheckErrorMessage(error: unknown): string {
  return extractErrorMessage(error) || String(error);
}
/**
 * 检查所有应用的环境变量冲突
 * @returns 按应用类型的检查结果（失败时携带 error，不伪装为空列表）
 */
export async function checkAllEnvConflicts(): Promise<
  Record<string, EnvConflictCheckResult>
> {
  const apps = ["claude", "codex"];
  const entries = await Promise.all(
    apps.map(async (app) => {
      try {
        const conflicts = await checkEnvConflicts(app);
        return [app, { ok: true as const, conflicts }] as const;
      } catch (error) {
        console.error(`检查 ${app} 环境变量失败:`, error);
        return [
          app,
          { ok: false as const, error: envCheckErrorMessage(error) },
        ] as const;
      }
    }),
  );
  return Object.fromEntries(entries);
}
/** 从批量检查结果中取出已成功应用的冲突（忽略失败项）。 */
export function flattenEnvConflicts(
  results: Record<string, EnvConflictCheckResult>,
): EnvConflict[] {
  return Object.values(results).flatMap((result) =>
    result.ok ? result.conflicts : [],
  );
}
/** 是否存在至少一个应用检查失败。 */
export function hasEnvCheckFailures(
  results: Record<string, EnvConflictCheckResult>,
): boolean {
  return Object.values(results).some((result) => !result.ok);
}
/** 汇总失败应用与错误文案，供 UI 横幅展示。 */
export function summarizeEnvCheckFailures(
  results: Record<string, EnvConflictCheckResult>,
): {
  apps: string[];
  message: string;
} | null {
  const failures = Object.entries(results).filter(
    (
      entry,
    ): entry is [
      string,
      Extract<
        EnvConflictCheckResult,
        {
          ok: false;
        }
      >,
    ] => !entry[1].ok,
  );
  if (failures.length === 0) return null;
  const apps = failures.map(([app]) => app);
  const message = failures
    .map(([app, result]) => `${app}: ${result.error}`)
    .join("\n");
  return { apps, message };
}
