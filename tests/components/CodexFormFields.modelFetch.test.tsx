import {
  fetchModelsForConfig,
  showFetchModelsError,
  type FetchedModel,
} from "@/lib/api/model-fetch";
import type { ComponentProps } from "react";
import {
  act,
  fireEvent,
  screen,
  waitFor,
  within,
} from "@testing-library/react";
import { FormProvider, useForm } from "react-hook-form";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { toast } from "sonner";
import { renderWithQueryClient as render } from "../utils/testQueryClient";
import { CodexFormFields } from "@/components/providers/forms/CodexFormFields";

vi.mock("@/lib/api/model-fetch", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/api/model-fetch")>()),
  fetchModelsForConfig: vi.fn(),

  modelFetchRequestHeaders: vi.fn(),
  showFetchModelsError: vi.fn(),
}));
vi.mock("sonner", () => ({
  toast: { error: vi.fn(), success: vi.fn(), info: vi.fn() },
}));

vi.mock("@/components/providers/forms/shared", async (importOriginal) => {
  const actual =
    await importOriginal<
      typeof import("@/components/providers/forms/shared")
    >();
  return {
    ...actual,
    ModelDropdown: ({ models }: { models: FetchedModel[] }) => (
      <output data-testid="model-options">
        {models.map((model) => model.id).join(",")}
      </output>
    ),
  };
});
type Props = ComponentProps<typeof CodexFormFields>;
type ProviderKind = "config";
function pendingModels() {
  let resolve!: (models: Array<FetchedModel>) => void;
  let reject!: (reason: Error) => void;
  const promise = new Promise<Array<FetchedModel>>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}
function advertisedModel(id = "model-1"): FetchedModel {
  return {
    id,
    ownedBy: "vendor",
  };
}
function makeProps(_kind: ProviderKind): Props {
  return {
    codexApiKey: "test-key",
    onApiKeyChange: vi.fn(),
    category: "third_party",
    shouldShowApiKeyLink: false,
    websiteUrl: "",
    shouldShowSpeedTest: false,
    codexBaseUrl: "https://example.com/v1",
    onBaseUrlChange: vi.fn(),
    isFullUrl: false,
    onFullUrlChange: vi.fn(),
    isEndpointModalOpen: false,
    onEndpointModalToggle: vi.fn(),
    autoSelect: false,
    onAutoSelectChange: vi.fn(),
    codexModel: "model-1",
    onModelChange: vi.fn(),
    apiFormat: "openai_chat",
    onApiFormatChange: vi.fn(),
    anthropicAuthField: "ANTHROPIC_AUTH_TOKEN",
    onAnthropicAuthFieldChange: vi.fn(),
    impersonateClaudeCode: false,
    onImpersonateClaudeCodeChange: vi.fn(),
    maxOutputTokens: "",
    onMaxOutputTokensChange: vi.fn(),
    promptCacheRouting: "auto",
    onPromptCacheRoutingChange: vi.fn(),
    catalogModels: [],
    onCatalogModelsChange: vi.fn(),
    speedTestEndpoints: [],
    customUserAgent: "",
    onCustomUserAgentChange: vi.fn(),
    localProxyHeadersOverride: "",
    onLocalProxyHeadersOverrideChange: vi.fn(),
    localProxyBodyOverride: "",
    onLocalProxyBodyOverrideChange: vi.fn(),
  };
}
function Harness(props: Props) {
  const form = useForm();
  return (
    <FormProvider {...form}>
      <CodexFormFields {...props} />
    </FormProvider>
  );
}
const fetchMockFor = (_kind: ProviderKind) => vi.mocked(fetchModelsForConfig);
// 拉到列表后按钮改叫「选择模型」并自动打开选择弹窗（弹窗外的元素被设为 aria-hidden）。
const fetchButton = () =>
  screen.getAllByRole("button", {
    name: /^providerForm\.(fetchModels|modelPicker\.title)$/,
    hidden: true,
  })[0];
const providerKinds: ProviderKind[] = ["config"];
describe("Codex model-fetch lifecycle", () => {
  beforeEach(() => {
    vi.resetAllMocks();
    vi.spyOn(console, "warn").mockImplementation(() => {});
  });
  afterEach(() => {
    vi.restoreAllMocks();
  });

  it.each(providerKinds)(
    "loads %s models through the existing provider route",
    async (kind) => {
      const pending = pendingModels();
      fetchMockFor(kind).mockReturnValue(pending.promise);
      const props = makeProps(kind);
      render(<Harness {...props} />);
      fireEvent.click(fetchButton());
      expect(fetchButton()).toBeDisabled();
      expect(fetchMockFor(kind)).toHaveBeenCalledTimes(1);
      {
        expect(fetchModelsForConfig).toHaveBeenCalledWith(
          props.codexBaseUrl,
          "test-key",
          false,
          undefined,
          "",
          { requestHeaders: undefined },
        );
      }
      await act(async () => pending.resolve([advertisedModel()]));
      await waitFor(() => expect(fetchButton()).toBeEnabled());
      expect(screen.getByTestId("model-options")).toHaveTextContent("model-1");
      // 列表上的按钮拉取成功后直接打开选择弹窗，不再弹成功提示
      expect(await screen.findByRole("dialog")).toBeInTheDocument();
      expect(toast.success).not.toHaveBeenCalled();
      expect(showFetchModelsError).not.toHaveBeenCalled();
      {
        expect(props.onCatalogModelsChange).not.toHaveBeenCalled();
      }
    },
  );
  it.each(providerKinds)(
    "keeps the empty-list notification for %s",
    async (kind) => {
      fetchMockFor(kind).mockResolvedValue([]);
      render(<Harness {...makeProps(kind)} />);
      fireEvent.click(fetchButton());
      await waitFor(() =>
        expect(toast.info).toHaveBeenCalledWith(
          "providerForm.fetchModelsEmpty",
        ),
      );
      expect(fetchButton()).toBeEnabled();
      expect(toast.success).not.toHaveBeenCalled();
    },
  );

  it.each(providerKinds)(
    "reports %s failures and releases loading state",
    async (kind) => {
      const error = new Error("model-list unavailable");
      fetchMockFor(kind).mockRejectedValue(error);
      render(<Harness {...makeProps(kind)} />);
      fireEvent.click(fetchButton());
      await waitFor(() =>
        expect(showFetchModelsError).toHaveBeenCalledWith(
          error,
          expect.any(Function),
        ),
      );
      expect(fetchButton()).toBeEnabled();
      expect(toast.success).not.toHaveBeenCalled();
      expect(console.warn).toHaveBeenCalledWith("[ModelFetch] Failed:", error);
    },
  );

  it.each(providerKinds)("preserves %s preflight validation", (kind) => {
    render(<Harness {...makeProps(kind)} codexApiKey="" codexBaseUrl="" />);
    fireEvent.click(fetchButton());
    expect(fetchMockFor(kind)).not.toHaveBeenCalled();
    expect(fetchButton()).toBeEnabled();
    if (kind === "config") {
      expect(showFetchModelsError).toHaveBeenCalledWith(
        null,
        expect.any(Function),
        { hasApiKey: false, hasBaseUrl: false },
      );
    } else {
      expect(toast.error).toHaveBeenCalledTimes(1);
    }
  });
  it("removes unchecked catalog rows and moves the default to the new first row", async () => {
    vi.mocked(fetchModelsForConfig).mockResolvedValue([
      { id: "model-1", ownedBy: null },
      { id: "model-2", ownedBy: null },
    ]);
    const props = {
      ...makeProps("config"),
      codexModel: "model-1",
      catalogModels: [
        { model: "model-1", contextWindow: 200000 },
        { model: "model-2", contextWindow: 128000 },
      ],
    };
    render(<Harness {...props} />);
    fireEvent.click(fetchButton());
    const dialog = await screen.findByRole("dialog");
    fireEvent.click(within(dialog).getByRole("checkbox", { name: "model-1" }));
    fireEvent.click(
      within(dialog).getByRole("button", {
        name: "providerForm.modelPicker.apply",
      }),
    );
    expect(props.onModelChange).toHaveBeenCalledWith("model-2");
    await waitFor(() =>
      expect(props.onCatalogModelsChange).toHaveBeenLastCalledWith([
        expect.objectContaining({ model: "model-2", contextWindow: 128000 }),
      ]),
    );
  });
  it("keeps the success notification for the default model fetch button without a model list", async () => {
    vi.mocked(fetchModelsForConfig).mockResolvedValue([advertisedModel()]);
    render(
      <Harness {...makeProps("config")} onCatalogModelsChange={undefined} />,
    );
    fireEvent.click(fetchButton());
    await waitFor(() =>
      expect(toast.success).toHaveBeenCalledWith(
        "providerForm.fetchModelsSuccess",
      ),
    );
    expect(screen.queryByRole("dialog")).not.toBeInTheDocument();
  });
});
