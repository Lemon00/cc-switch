import { invoke } from "@tauri-apps/api/core";
import { describe, expect, it, vi } from "vitest";
import { mcpApi } from "@/lib/api/mcp";
vi.mock("@tauri-apps/api/core", () => ({ invoke: vi.fn() }));
describe("legacy MCP response contract", () => {
  it("maps the Rust path field and preserves raw server specifications", async () => {
    const servers = {
      local: { command: "node", args: ["server.js"] },
      remote: { type: "http", url: "https://example.test/mcp" },
    };
    vi.mocked(invoke).mockResolvedValueOnce({
      config_path: "/config/cc-switch/config.json",
      servers,
    });
    const response = await mcpApi.getConfig("codex");
    expect(invoke).toHaveBeenCalledWith("get_mcp_config", { app: "codex" });
    expect(response.configPath).toBe("/config/cc-switch/config.json");
    expect(response.servers).toEqual(servers);
    expect(response.servers.local.command).toBe("node");
  });
  it("propagates IPC failures instead of returning an empty configuration", async () => {
    vi.mocked(invoke).mockRejectedValueOnce("database unavailable");
    await expect(mcpApi.getConfig()).rejects.toBe("database unavailable");
  });
});
