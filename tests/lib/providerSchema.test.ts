import { describe, expect, it } from "vitest";
import { providerSchema } from "@/lib/schemas/provider";
describe("provider configuration root", () => {
  it.each(["null", "[]", "123", '"text"', "true"])(
    "rejects %s before submission",
    (settingsConfig) => {
      expect(
        providerSchema.safeParse({ name: "Provider", settingsConfig }).success,
      ).toBe(false);
    },
  );
  it("accepts native configuration objects", () => {
    expect(
      providerSchema.safeParse({
        name: "Provider",
        settingsConfig: '{"env":{}}',
      }).success,
    ).toBe(true);
  });
});
