import { afterEach, describe, expect, it, vi } from "vitest";
import { readLocalCache, writeLocalCache } from "@/lib/localCache";
import { isPromptCache, isSkillDiscoveryCache } from "@/lib/cacheValidation";
const isStringList = (value: unknown): value is string[] =>
  Array.isArray(value) && value.every((item) => typeof item === "string");
afterEach(() => vi.restoreAllMocks());
describe("local cache boundaries", () => {
  it.each(["null", "42", "{}", '["valid",42]', "{bad json"])(
    "ignores incompatible cache %s",
    (raw) => {
      localStorage.setItem("cc-switch-cache:fixture", raw);
      expect(readLocalCache("fixture", isStringList)).toBeUndefined();
    },
  );
  it("accepts empty and populated valid caches", () => {
    for (const data of [[], ["one"]]) {
      writeLocalCache("fixture", data);
      expect(readLocalCache("fixture", isStringList)).toEqual(data);
    }
  });
  it("validates nested prompt and discovery fields before exposing cached data", () => {
    const prompt = { id: "one", name: "One", content: "text", enabled: false };
    const skill = {
      key: "a/b:one",
      name: "One",
      description: "",
      directory: "one",
      repoOwner: "a",
      repoName: "b",
      repoBranch: "main",
    };
    expect(isPromptCache({ one: prompt })).toBe(true);
    expect(isPromptCache({ one: { ...prompt, content: 42 } })).toBe(false);
    expect(isPromptCache({ one: { ...prompt, id: "other" } })).toBe(false);
    expect(isPromptCache({})).toBe(true);
    expect(isSkillDiscoveryCache({ skills: [skill], failures: [] })).toBe(true);
    expect(
      isSkillDiscoveryCache({
        skills: [skill],
        failures: [{ owner: "a", name: "b", branch: "main", error: "offline" }],
      }),
    ).toBe(true);
    expect(isSkillDiscoveryCache({ skills: null, failures: [] })).toBe(false);
    expect(
      isSkillDiscoveryCache({
        skills: [{ ...skill, directory: null }],
        failures: [],
      }),
    ).toBe(false);
    expect(isSkillDiscoveryCache({ skills: [], failures: [42] })).toBe(false);
  });
  it("tolerates denied reads and writes", () => {
    vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
      throw new DOMException("denied", "SecurityError");
    });
    vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
      throw new DOMException("full", "QuotaExceededError");
    });
    expect(readLocalCache("fixture", isStringList)).toBeUndefined();
    expect(() => writeLocalCache("fixture", [])).not.toThrow();
  });
});
