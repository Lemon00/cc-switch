import { describe, expect, it, vi } from "vitest";
import {
  extractErrorMessage,
  translateMcpBackendError,
} from "@/utils/errorUtils";

describe("error utilities", () => {
  it("extracts Tauri string errors", () => {
    expect(extractErrorMessage("backend failed")).toBe("backend failed");
  });
  it("translates validation errors for the retained MCP clients", () => {
    const t = vi.fn((key: string) => key);
    expect(translateMcpBackendError("MCP 服务器 ID 不能为空", t)).toBe(
      "mcp.error.idRequired",
    );
    expect(translateMcpBackendError("unrecognized failure", t)).toBe("");
  });
});
