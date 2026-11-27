import { render, screen, waitFor } from "@testing-library/react";
import userEvent from "@testing-library/user-event";
import { QueryClientProvider } from "@tanstack/react-query";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { http, HttpResponse } from "msw";
import type { Settings } from "@/types";
import type { ProxyStackSubagentModel } from "@/types/proxy";
import { StackSettingsSheet } from "@/components/providers/mode/StackSettingsSheet";
import { server } from "../msw/server";
import { getSettings, setSettings } from "../msw/state";
import { createTestQueryClient } from "../utils/testQueryClient";
const TAURI_ENDPOINT = "http://tauri.local";
const MODELS: ProxyStackSubagentModel[] = [
  {
    id: "group/shared-model",
    aliases: ["ccs-account-a/shared-model", "account-a/shared-model"],
    providerId: "",
    displayName: "group/shared-model",
    efforts: ["low", "high"],
    defaultEffort: "high",
  },
  {
    id: "account-b/model-b",
    providerId: "account-b",
    displayName: "account-b/model-b",
    efforts: ["medium"],
  },
];
const MODEL = "mode.stackSettings.subagentModel";
const EFFORT = "mode.stackSettings.subagentEffort";
const CLASSIC = "settings.codexStackClassicSubagents";
const AUTO_REVIEW = "mode.stackSettings.autoReviewModel";
type SettingsPatch = {
  [Key in keyof Settings]?: Settings[Key] | null;
};
let saved: SettingsPatch[] = [];
function renderSheet(
  settings: Partial<Settings> = {},
  {
    routeOfficial = false,
  }: {
    routeOfficial?: boolean;
  } = {},
) {
  setSettings(settings);
  saved = [];
  server.use(
    http.post(`${TAURI_ENDPOINT}/save_settings`, async ({ request }) => {
      const { settings: next } = (await request.json()) as {
        settings: SettingsPatch;
      };
      saved.push(next);
      setSettings(next);
      return HttpResponse.json(true);
    }),
  );
  return render(
    <QueryClientProvider client={createTestQueryClient()}>
      <StackSettingsSheet
        open
        onOpenChange={vi.fn()}
        models={MODELS}
        routeOfficial={routeOfficial}
      />
    </QueryClientProvider>,
  );
}
beforeEach(() => {
  server.use(
    http.post(
      `${TAURI_ENDPOINT}/get_proxy_config_for_app`,
      async ({ request }) => {
        const { appType } = (await request.json()) as {
          appType: string;
        };
        return HttpResponse.json({
          appType,
          enabled: true,
          autoFailoverEnabled: false,
          maxRetries: 3,
          streamingFirstByteTimeout: 60,
          streamingIdleTimeout: 120,
          nonStreamingTimeout: 600,
          circuitFailureThreshold: 5,
          circuitSuccessThreshold: 2,
          circuitTimeoutSeconds: 60,
          circuitErrorRateThreshold: 0.5,
          circuitMinRequests: 10,
        });
      },
    ),
  );
  // Radix Select 打开时会滚动到选中项，jsdom 没有实现
  Element.prototype.scrollIntoView = vi.fn();
});
describe("StackSettingsSheet", () => {
  it("does not overwrite saved subagent settings while the initial read is pending", async () => {
    let release!: () => void;
    const pending = new Promise<void>((resolve) => {
      release = resolve;
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_settings`, async () => {
        await pending;
        return HttpResponse.json(getSettings());
      }),
    );
    const user = userEvent.setup();
    renderSheet({
      codexStackSubagentModel: "group/shared-model",
      codexStackSubagentReasoningEffort: "high",
      codexStackClassicTools: true,
    });
    try {
      expect(screen.getByRole("combobox", { name: MODEL })).toBeDisabled();
      expect(
        screen.getByRole("combobox", { name: AUTO_REVIEW }),
      ).toBeDisabled();
      expect(screen.getByRole("switch", { name: CLASSIC })).toBeDisabled();
      await user.click(screen.getByRole("switch", { name: CLASSIC }));
      expect(saved).toEqual([]);
    } finally {
      release();
    }
    await waitFor(() => {
      expect(screen.getByRole("combobox", { name: MODEL })).toHaveTextContent(
        "group/shared-model",
      );
      expect(screen.getByRole("combobox", { name: EFFORT })).toHaveTextContent(
        "high",
      );
    });
    await user.click(screen.getByRole("switch", { name: CLASSIC }));
    await waitFor(() =>
      expect(saved).toEqual([
        {
          codexStackSubagentModel: "group/shared-model",
          codexStackSubagentReasoningEffort: "high",
          codexStackClassicTools: false,
        },
      ]),
    );
  });
  it.each(["ccs-account-a/shared-model", "account-a/shared-model"])(
    "shows saved selection %s as the shared model for subagents and review",
    async (alias) => {
      renderSheet({
        codexStackSubagentModel: alias,
        codexStackAutoReviewModel: alias,
      });
      await waitFor(() => {
        expect(screen.getByRole("combobox", { name: MODEL })).toHaveTextContent(
          "group/shared-model",
        );
        expect(
          screen.getByRole("combobox", { name: AUTO_REVIEW }),
        ).toHaveTextContent("group/shared-model");
      });
      expect(
        screen.queryByTestId("stack-subagent-unavailable"),
      ).not.toBeInTheDocument();
      expect(
        screen.queryByTestId("stack-auto-review-unavailable"),
      ).not.toBeInTheDocument();
    },
  );
  it("treats the classic tools as on when the setting was never saved", async () => {
    renderSheet();
    expect(await screen.findByRole("switch", { name: CLASSIC })).toBeChecked();
  });
  it("saves the picked sub-agent model and warns while classic tools are off", async () => {
    const user = userEvent.setup();
    renderSheet({ codexStackClassicTools: false });
    const model = await screen.findByRole("combobox", { name: MODEL });
    expect(model).toHaveTextContent("mode.stackSettings.followMain");
    // 跟随主模型时没有强度可选
    expect(screen.getByRole("combobox", { name: EFFORT })).toBeDisabled();
    await user.click(model);
    await user.click(
      await screen.findByRole("option", {
        name: "group/shared-model",
      }),
    );
    await waitFor(() =>
      expect(getSettings().codexStackSubagentModel).toBe("group/shared-model"),
    );
    expect(getSettings().codexStackSubagentReasoningEffort).toBeUndefined();
    expect(
      await screen.findByTestId("stack-subagent-classic-off"),
    ).toBeInTheDocument();
  });
  it("offers only the picked model's efforts and drops one the next model lacks", async () => {
    const user = userEvent.setup();
    renderSheet({
      codexStackSubagentModel: "group/shared-model",
      codexStackClassicTools: true,
    });
    const effort = await screen.findByRole("combobox", { name: EFFORT });
    await waitFor(() => expect(effort).not.toBeDisabled());
    expect(
      screen.queryByTestId("stack-subagent-classic-off"),
    ).not.toBeInTheDocument();
    await user.click(effort);
    const options = await screen.findAllByRole("option");
    expect(options.map((option) => option.textContent)).toEqual([
      "mode.stackSettings.effortModelDefaultNamed",
      "low",
      "high",
    ]);
    await user.click(screen.getByRole("option", { name: "low" }));
    await waitFor(() =>
      expect(getSettings().codexStackSubagentReasoningEffort).toBe("low"),
    );
    // 账号 B 的模型没有 low：换过去时清掉强度
    await user.click(screen.getByRole("combobox", { name: MODEL }));
    await user.click(
      await screen.findByRole("option", { name: "account-b/model-b" }),
    );
    await waitFor(() =>
      expect(getSettings().codexStackSubagentModel).toBe("account-b/model-b"),
    );
    expect(getSettings().codexStackSubagentReasoningEffort).toBeUndefined();
  });
  it("explains a saved model that is no longer aggregated", async () => {
    renderSheet({ codexStackSubagentModel: "ccs-gone/old-model" });
    expect(
      await screen.findByTestId("stack-subagent-unavailable"),
    ).toBeInTheDocument();
    expect(screen.getByRole("combobox", { name: MODEL })).toHaveTextContent(
      "mode.stackSettings.unavailableOption",
    );
  });
  it("goes back to following the main model", async () => {
    const user = userEvent.setup();
    renderSheet({
      codexStackSubagentModel: "account-b/model-b",
      codexStackSubagentReasoningEffort: "medium",
    });
    const model = await screen.findByRole("combobox", { name: MODEL });
    await waitFor(() => expect(model).toHaveTextContent("account-b/model-b"));
    await user.click(model);
    await user.click(
      await screen.findByRole("option", {
        name: "mode.stackSettings.followMain",
      }),
    );
    await waitFor(() =>
      expect(getSettings().codexStackSubagentModel).toBeUndefined(),
    );
    expect(getSettings().codexStackSubagentReasoningEffort).toBeUndefined();
    const last = saved.at(-1)!;
    expect(last.codexStackSubagentModel).toBeNull();
    expect(last.codexStackSubagentReasoningEffort).toBeNull();
  });
  it("moves the classic sub-agent toggle here and warns when config.toml forces v2", async () => {
    const user = userEvent.setup();
    server.use(
      http.post(`${TAURI_ENDPOINT}/codex_forces_multi_agent_v2`, () =>
        HttpResponse.json(true),
      ),
    );
    renderSheet({ codexStackClassicTools: false });
    const toggle = await screen.findByRole("switch", { name: CLASSIC });
    // 设置读回来之前按默认（开）显示
    await waitFor(() =>
      expect(toggle).toHaveAttribute("aria-checked", "false"),
    );
    // 关着时不提示
    expect(
      screen.queryByText("settings.codexStackClassicSubagentsForcedV2"),
    ).not.toBeInTheDocument();
    await user.click(toggle);
    await waitFor(() =>
      expect(getSettings().codexStackClassicTools).toBe(true),
    );
    expect(
      await screen.findByText("settings.codexStackClassicSubagentsForcedV2"),
    ).toBeInTheDocument();
  });
  it("labels the default reviewer by the default provider", async () => {
    const { unmount } = renderSheet({}, { routeOfficial: true });
    expect(
      await screen.findByRole("combobox", { name: AUTO_REVIEW }),
    ).toHaveTextContent("mode.stackSettings.autoReviewDefaultOfficial");
    unmount();
    renderSheet();
    expect(
      await screen.findByRole("combobox", { name: AUTO_REVIEW }),
    ).toHaveTextContent("mode.stackSettings.autoReviewDefaultMain");
  });
  it("saves the picked Auto Review model and goes back to the default", async () => {
    const user = userEvent.setup();
    renderSheet({ codexStackSubagentModel: "account-b/model-b" });
    const reviewer = await screen.findByRole("combobox", {
      name: AUTO_REVIEW,
    });
    await user.click(reviewer);
    await user.click(
      await screen.findByRole("option", {
        name: "group/shared-model",
      }),
    );
    await waitFor(() =>
      expect(getSettings().codexStackAutoReviewModel).toBe(
        "group/shared-model",
      ),
    );
    expect(saved.at(-1)).not.toHaveProperty("codexStackSubagentModel");
    expect(getSettings().codexStackSubagentModel).toBe("account-b/model-b");
    await user.click(screen.getByRole("combobox", { name: AUTO_REVIEW }));
    await user.click(
      await screen.findByRole("option", {
        name: "mode.stackSettings.autoReviewDefaultMain",
      }),
    );
    await waitFor(() =>
      expect(getSettings().codexStackAutoReviewModel).toBeUndefined(),
    );
    expect(saved.at(-1)!.codexStackAutoReviewModel).toBeNull();
  });
  it("explains a saved Auto Review model that is no longer aggregated", async () => {
    renderSheet({ codexStackAutoReviewModel: "ccs-gone/old-model" });
    expect(
      await screen.findByTestId("stack-auto-review-unavailable"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("combobox", { name: AUTO_REVIEW }),
    ).toHaveTextContent("mode.stackSettings.unavailableOption");
    expect(
      screen.queryByTestId("stack-subagent-unavailable"),
    ).not.toBeInTheDocument();
  });
});
