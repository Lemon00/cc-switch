import { describe, expect, it } from "vitest";
import {
  getCacheWriteAvailability,
  isNonNegativeDecimalString,
} from "@/types/usage";
describe("getCacheWriteAvailability", () => {
  it("rejects prices outside the backend calculation range", () => {
    expect(isNonNegativeDecimalString("1000000000000")).toBe(false);
    expect(isNonNegativeDecimalString("79228162514264337593543950335")).toBe(
      false,
    );
    expect(isNonNegativeDecimalString("0.000001")).toBe(true);
    expect(isNonNegativeDecimalString("0")).toBe(true);
  });
  it("distinguishes cache-write support across fixed protocols", () => {
    expect(getCacheWriteAvailability(["claude"])).toBe("ok");
    // Retired clients can still occur in historical usage records.
    expect(getCacheWriteAvailability(["pi", "omp", "mcode", "kimi"])).toBe(
      "partial",
    );
    expect(getCacheWriteAvailability(["codex"])).toBe("na");
    expect(getCacheWriteAvailability(["claude", "codex"])).toBe("partial");
    expect(getCacheWriteAvailability([])).toBe("ok");
  });
});
