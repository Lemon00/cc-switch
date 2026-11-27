import { isPlainObject } from "@/lib/requestOverrides";
// Keys only a V1 declaration has, and keys only a native one has. Mirrors
// provider_format in crates/cc-switch-core/src/opencode_config.rs.
const OPENCODE_LEGACY_ONLY_KEYS = ["npm", "options", "api"];
const OPENCODE_NATIVE_ONLY_KEYS = [
  "package",
  "settings",
  "headers",
  "body",
  "canonical",
];
function parseJson(json: string): unknown {
  try {
    return JSON.parse(json);
  } catch {
    return undefined;
  }
}
/** Keep native declarations out of the V1 structured editor. Source metadata
 * disambiguates built-in overrides containing only models (or an empty object).
 * A pasted full config holding `providers` counts as native too.
 */
export function isNativeOpencodeConfig(
  json: string,
  source?: "v1" | "v2",
): boolean {
  if (source) return source === "v2";
  const value = parseJson(json);
  if (!isPlainObject(value)) return false;
  if (OPENCODE_LEGACY_ONLY_KEYS.some((key) => key in value)) return false;
  return [...OPENCODE_NATIVE_ONLY_KEYS, "providers"].some(
    (key) => key in value,
  );
}
/** A declaration that does not rely on a built-in definition: it names a
 * package (`npm` for V1, `package` for native) and at least one model.
 */
export function hasOpencodeDefinition(
  declaration: unknown,
  packageKey: "npm" | "package",
): boolean {
  if (!isPlainObject(declaration)) return false;
  const pkg = declaration[packageKey];
  const { models } = declaration;
  return (
    typeof pkg === "string" &&
    pkg.trim() !== "" &&
    isPlainObject(models) &&
    Object.keys(models).length > 0
  );
}
/** The native form's check: a pasted full config holds the declaration under
 * `providers.<id>`, where the backend also takes it from.
 */
export function hasNativeOpencodeDefinition(
  json: string,
  providerId: string,
): boolean {
  const value = parseJson(json);
  const providers = isPlainObject(value) ? value.providers : undefined;
  const declaration = isPlainObject(providers)
    ? (providers[providerId] ?? value)
    : value;
  return hasOpencodeDefinition(declaration, "package");
}
/** Models a stored provider declares, read from the fragment the backend
 * writes: settings may hold a full config with the declaration under
 * `providers.<id>` / `provider.<id>`.
 */
export function getOpencodeDeclaredModels(
  providerId: string,
  settingsConfig: unknown,
): {
  id: string;
  name?: string;
}[] {
  let declaration = settingsConfig;
  if (
    isPlainObject(settingsConfig) &&
    ["$schema", "provider", "providers"].some((key) => key in settingsConfig)
  ) {
    const pick = (key: string) => {
      const section = settingsConfig[key];
      return isPlainObject(section) ? section[providerId] : undefined;
    };
    declaration = pick("providers") ?? pick("provider") ?? settingsConfig;
  }
  const models = isPlainObject(declaration) ? declaration.models : undefined;
  if (!isPlainObject(models)) return [];
  return (
    Object.entries(models)
      // 原生声明可以 `disabled: true` 关掉某个模型，不能拿它当默认
      .filter(
        ([id, model]) =>
          id.trim() && !(isPlainObject(model) && model.disabled === true),
      )
      .map(([id, model]) => ({
        id,
        name:
          isPlainObject(model) && typeof model.name === "string"
            ? model.name
            : undefined,
      }))
  );
}
