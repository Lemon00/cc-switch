import { fireEvent, render, screen, waitFor } from "@testing-library/react";
import { QueryClientProvider } from "@tanstack/react-query";
import { describe, expect, it, vi } from "vitest";
import {
  ProviderForm,
  type ProviderFormValues,
} from "@/components/providers/forms/ProviderForm";
import { createTestQueryClient } from "../utils/testQueryClient";
vi.mock("@/components/JsonEditor", () => ({ default: () => <div /> }));
vi.mock("@/components/providers/forms/CodexConfigEditor", () => ({
  default: () => <div />,
}));
vi.mock("@/components/providers/forms/hooks", async (importOriginal) => {
  const actual =
    await importOriginal<typeof import("@/components/providers/forms/hooks")>();
  const signedOut = {
    isAuthenticated: false,
    isStatusSuccess: true,
    isStatusError: false,
    accounts: [],
  };
  return {
    ...actual,
    useCopilotAuth: () => signedOut,
    useCodexOauth: () => signedOut,
    useXaiOauth: () => signedOut,
  };
});
describe("Gemini upstream configuration", () => {
  it.each(["claude", "codex"] as const)(
    "%s reopens and saves Gemini format and full URL",
    async (appId) => {
      const onSubmit = vi.fn();
      const settingsConfig =
        appId === "claude"
          ? {
              env: {
                ANTHROPIC_BASE_URL: "https://fixture.example/v1beta",
                ANTHROPIC_AUTH_TOKEN: "fixture-key",
                ANTHROPIC_MODEL: "gemini-flash",
              },
            }
          : {
              auth: {},
              config:
                'model_provider = "custom"\nmodel = "gemini-flash"\n[model_providers.custom]\nname = "custom"\nbase_url = "https://fixture.example/v1beta"\nexperimental_bearer_token = "fixture-key"\nwire_api = "responses"\n',
            };
      render(
        <QueryClientProvider client={createTestQueryClient()}>
          <ProviderForm
            appId={appId}
            providerId="gemini-relay"
            submitLabel="save-fixture"
            onSubmit={onSubmit}
            onCancel={vi.fn()}
            initialData={{
              name: "Gemini Relay",
              category: "third_party",
              settingsConfig,
              meta: { apiFormat: "gemini_native", isFullUrl: true },
            }}
          />
        </QueryClientProvider>,
      );
      await screen.findByRole("button", { name: "save-fixture" });
      const advanced = screen.queryByRole("button", {
        name: "providerForm.advancedOptionsToggle",
      });
      if (advanced?.getAttribute("aria-expanded") === "false")
        fireEvent.click(advanced);
      expect(
        document.getElementById(
          appId === "claude" ? "apiFormat" : "codex-upstream-format",
        )?.textContent,
      ).toContain("Gemini");
      expect(screen.getByRole("switch", { name: "完整 URL" })).toHaveAttribute(
        "aria-checked",
        "true",
      );
      fireEvent.click(screen.getByRole("button", { name: "save-fixture" }));
      await waitFor(() => expect(onSubmit).toHaveBeenCalledOnce());
      const saved = onSubmit.mock.calls[0][0] as ProviderFormValues;
      expect(saved.meta).toMatchObject({
        apiFormat: "gemini_native",
        isFullUrl: true,
      });
      expect(JSON.parse(saved.settingsConfig)).toMatchObject(
        appId === "claude"
          ? { env: { ANTHROPIC_AUTH_TOKEN: "fixture-key" } }
          : { auth: {} },
      );
    },
  );
  it("OpenCode reopens and saves the Google SDK and model options", async () => {
    const onSubmit = vi.fn();
    const settingsConfig = {
      npm: "@ai-sdk/google",
      name: "Google",
      options: {
        baseURL: "https://fixture.example/v1beta",
        apiKey: "fixture-key",
      },
      models: {
        "gemini-flash": {
          name: "Gemini Flash",
          limit: { context: 1048576 },
          options: { thinkingConfig: { thinkingBudget: 1024 } },
        },
      },
    };
    render(
      <QueryClientProvider client={createTestQueryClient()}>
        <ProviderForm
          appId="opencode"
          providerId="google-relay"
          submitLabel="save-fixture"
          onSubmit={onSubmit}
          onCancel={vi.fn()}
          initialData={{
            name: "Google",
            category: "third_party",
            settingsConfig,
          }}
        />
      </QueryClientProvider>,
    );
    const save = await screen.findByRole("button", { name: "save-fixture" });
    await waitFor(() => expect(save).toBeEnabled());
    fireEvent.click(save);
    await waitFor(() => expect(onSubmit).toHaveBeenCalledOnce());
    expect(JSON.parse(onSubmit.mock.calls[0][0].settingsConfig)).toMatchObject(
      settingsConfig,
    );
  });
});
