import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { QueryClient, QueryClientProvider } from "@tanstack/react-query";
import { describe, it, expect, vi, beforeEach } from "vitest";
import { useState, type ReactElement } from "react";
import { http, HttpResponse } from "msw";
import type { Provider } from "@/types";
import {
  ProviderList,
  type SwitchModeProps,
} from "@/components/providers/ProviderList";
import { server } from "../msw/server";
vi.mock("sonner", () => ({
  toast: {
    success: vi.fn(),
    info: vi.fn(),
    warning: vi.fn(),
    error: vi.fn(),
  },
}));
const TAURI_ENDPOINT = "http://tauri.local";
const useDragSortMock = vi.fn();
const useSortableMock = vi.fn();
const providerCardRenderSpy = vi.fn();
/** 某张卡片最近一次渲染拿到的 props。 */
const lastProps = (id: string) =>
  providerCardRenderSpy.mock.calls
    .map((call) => call[0])
    .filter((props) => props.provider.id === id)
    .at(-1);
const dshAccountStatusMock = vi.fn();
vi.mock("@/components/providers/DshAccount", () => ({
  useDshAccountStatus: (enabled: boolean) =>
    enabled ? dshAccountStatusMock() : { data: undefined },
  accountName: (status: any, fallback: string) =>
    status.profile?.name ?? fallback,
  DshAccountCard: ({ status }: any) => (
    <div data-testid="dsh-account-card">{status.profile?.name}</div>
  ),
}));
vi.mock("@/hooks/useDragSort", () => ({
  useDragSort: (...args: unknown[]) => useDragSortMock(...args),
}));
vi.mock("@/components/providers/ProviderCard", () => ({
  ProviderCard: (props: any) => {
    providerCardRenderSpy(props);
    const { provider, presentation, onEdit, onDelete, onDuplicate } = props;
    return (
      <div data-testid={`provider-card-${provider.id}`}>
        {presentation.status && (
          <span data-testid={`status-${provider.id}`}>
            {presentation.status.label}
          </span>
        )}
        {presentation.buttons.map((button: any) => (
          <button
            key={button.key}
            data-testid={`${button.key}-${provider.id}`}
            disabled={Boolean(button.disabledReason)}
            onClick={button.onClick}
          >
            {button.label}
          </button>
        ))}
        <button
          data-testid={`edit-${provider.id}`}
          onClick={() => onEdit(provider)}
        >
          edit
        </button>
        <button
          data-testid={`duplicate-${provider.id}`}
          onClick={() => onDuplicate(provider)}
        >
          duplicate
        </button>
        <button
          data-testid={`delete-${provider.id}`}
          onClick={() => onDelete(provider)}
        >
          delete
        </button>
        <span data-testid={`drag-attr-${provider.id}`}>
          {props.dragHandleProps?.attributes?.["data-dnd-id"] ?? "none"}
        </span>
      </div>
    );
  },
}));
vi.mock("@/components/UsageFooter", () => ({
  default: () => <div data-testid="usage-footer" />,
}));
vi.mock("@dnd-kit/sortable", async () => {
  const actual = await vi.importActual<any>("@dnd-kit/sortable");
  return {
    ...actual,
    useSortable: (...args: unknown[]) => useSortableMock(...args),
  };
});
// Mock hooks that use QueryClient
vi.mock("@/hooks/useStreamCheck", () => ({
  useStreamCheck: () => ({
    checkProvider: vi.fn(),
    isChecking: () => false,
  }),
}));
function createProvider(overrides: Partial<Provider> = {}): Provider {
  return {
    id: overrides.id ?? "provider-1",
    name: overrides.name ?? "Test Provider",
    settingsConfig: overrides.settingsConfig ?? {},
    category: overrides.category,
    createdAt: overrides.createdAt,
    sortIndex: overrides.sortIndex,
    meta: overrides.meta,
    websiteUrl: overrides.websiteUrl,
  };
}
function renderWithQueryClient(ui: ReactElement) {
  const queryClient = new QueryClient({
    defaultOptions: { queries: { retry: false } },
  });
  return {
    ...render(
      <QueryClientProvider client={queryClient}>{ui}</QueryClientProvider>,
    ),
    queryClient,
  };
}
beforeEach(() => {
  useDragSortMock.mockReset();
  useSortableMock.mockReset();
  providerCardRenderSpy.mockClear();
  useSortableMock.mockImplementation(({ id }: { id: string }) => ({
    setNodeRef: vi.fn(),
    attributes: { "data-dnd-id": id },
    listeners: { onPointerDown: vi.fn() },
    transform: null,
    transition: null,
    isDragging: false,
  }));
  useDragSortMock.mockReturnValue({
    sortedProviders: [],
    sensors: [],
    handleDragEnd: vi.fn(),
  });
});
describe("ProviderList Component", () => {
  const baseProps = {
    currentProviderId: "",
    onSwitch: vi.fn(),
    onEdit: vi.fn(),
    onDelete: vi.fn(),
    onDuplicate: vi.fn(),
    onOpenWebsite: vi.fn(),
  };
  function switchMode(
    overrides: Partial<SwitchModeProps> = {},
  ): SwitchModeProps {
    return {
      active: "direct",
      view: "direct",
      directId: null,
      routeId: null,
      failoverOn: false,
      queue: [],
      stackMembers: new Map(),
      routingReason: () => "",
      serviceRunning: false,
      actions: {
        switchDirect: vi.fn(),
        needsRouteDialog: vi.fn(),
        exitAndUse: vi.fn(),
        routeTo: vi.fn(),
        queueAdd: vi.fn(),
        queueRemove: vi.fn(),
        queueMove: vi.fn(),
        stackAdd: vi.fn(),
        stackRemove: vi.fn(),
        stackSetDefault: vi.fn(),
      },
      ...overrides,
    };
  }
  it("should render skeleton placeholders when loading", () => {
    const { container } = renderWithQueryClient(
      <ProviderList {...baseProps} providers={{}} appId="claude" isLoading />,
    );
    expect(container.querySelectorAll(".border-dashed")).toHaveLength(3);
  });
  it("should show empty state and trigger create callback when no providers exist", () => {
    const handleCreate = vi.fn();
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{}}
        appId="claude"
        onCreate={handleCreate}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "provider.noProviders" }),
    ).toBeInTheDocument();
    // 页头已经有实心的「添加供应商」，空状态里的两个按钮都是描边
    const importButton = screen.getByRole("button", {
      name: "provider.importCurrent",
    });
    const addButton = screen.getByRole("button", {
      name: "provider.addProvider",
    });
    for (const button of [importButton, addButton]) {
      expect(button.className).toContain("border-border-strong");
      expect(button.className).not.toContain("bg-action");
    }
    fireEvent.click(addButton);
    expect(handleCreate).toHaveBeenCalledTimes(1);
  });
  const serveOneUnreadableProvider = () =>
    server.use(
      http.post("http://tauri.local/get_unreadable_providers", () =>
        HttpResponse.json([
          {
            id: "broken",
            name: "Broken",
            error: "供应商 broken 的 meta 无法解析",
          },
        ]),
      ),
    );
  it("lists unreadable providers apart, even when no readable one is left", async () => {
    serveOneUnreadableProvider();
    renderWithQueryClient(
      <ProviderList {...baseProps} providers={{}} appId="claude" />,
    );
    expect(
      await screen.findByText("「Broken」的数据无法读取"),
    ).toBeInTheDocument();
    expect(
      screen.getByRole("heading", { name: "provider.noProviders" }),
    ).toBeInTheDocument();
  });
  it("lists unreadable providers apart next to the readable ones", async () => {
    serveOneUnreadableProvider();
    const provider = createProvider({ id: "a", name: "A" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [provider],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{ a: provider }}
        appId="claude"
      />,
    );
    expect(
      await screen.findByText("「Broken」的数据无法读取"),
    ).toBeInTheDocument();
    expect(
      providerCardRenderSpy.mock.calls.map(([p]) => p.provider.id),
    ).toEqual(expect.arrayContaining(["a"]));
  });
  it("renders in the order returned by useDragSort and wires the mode actions", () => {
    const providerA = createProvider({ id: "a", name: "A" });
    const providerB = createProvider({ id: "b", name: "B" });
    const handleEdit = vi.fn();
    const handleDelete = vi.fn();
    const handleDuplicate = vi.fn();
    const mode = switchMode({ directId: "b" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [providerB, providerA],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{ a: providerA, b: providerB }}
        currentProviderId="b"
        appId="claude"
        onEdit={handleEdit}
        onDelete={handleDelete}
        onDuplicate={handleDuplicate}
        switchMode={mode}
      />,
    );
    expect(providerCardRenderSpy).toHaveBeenCalledTimes(2);
    expect(providerCardRenderSpy.mock.calls[0][0].provider.id).toBe("b");
    expect(providerCardRenderSpy.mock.calls[1][0].provider.id).toBe("a");
    expect(lastProps("b")?.isCurrent).toBe(true);
    expect(screen.getByTestId("drag-attr-b")).toHaveTextContent("b");
    expect(screen.getByTestId("drag-attr-a")).toHaveTextContent("a");
    // 直连那家显示「使用中」，其余是「切换」
    expect(screen.getByTestId("status-b")).toHaveTextContent(
      "providerCard.status.inUse",
    );
    fireEvent.click(screen.getByTestId("switch-a"));
    expect(mode.actions.switchDirect).toHaveBeenCalledWith(providerA);
    fireEvent.click(screen.getByTestId("edit-b"));
    fireEvent.click(screen.getByTestId("duplicate-b"));
    fireEvent.click(screen.getByTestId("delete-a"));
    expect(handleEdit).toHaveBeenCalledWith(providerB);
    expect(handleDuplicate).toHaveBeenCalledWith(providerB);
    expect(handleDelete).toHaveBeenCalledWith(providerA);
    expect(useDragSortMock).toHaveBeenCalledWith(
      { a: providerA, b: providerB },
      "claude",
    );
  });
  it("shows queue sections while failover is on", () => {
    const providerA = createProvider({ id: "a", name: "A" });
    const providerB = createProvider({ id: "b", name: "B" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [providerA, providerB],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{ a: providerA, b: providerB }}
        appId="claude"
        switchMode={switchMode({
          active: "route",
          view: "route",
          routeId: "b",
          failoverOn: true,
          queue: ["b"],
        })}
      />,
    );
    expect(
      screen.getByRole("heading", { name: "providerCard.section.queue" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("queueRemove-b")).toBeInTheDocument();
    expect(screen.getByTestId("queueAdd-a")).toBeInTheDocument();
  });
  it("uses the additive layout for apps that keep several providers", async () => {
    const providerA = createProvider({ id: "a", name: "A" });
    const live = createProvider({ id: "live", name: "Live" });
    const onSwitch = vi.fn();
    const onRemoveFromConfig = vi.fn();
    useDragSortMock.mockReturnValue({
      sortedProviders: [providerA, live],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_opencode_live_provider_ids`, () =>
        HttpResponse.json(["live"]),
      ),
    );
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{ a: providerA, live }}
        appId="opencode"
        onSwitch={onSwitch}
        onRemoveFromConfig={onRemoveFromConfig}
      />,
    );
    fireEvent.click(await screen.findByTestId("remove-live"));
    expect(onRemoveFromConfig).toHaveBeenCalledWith(live);
    fireEvent.click(screen.getByTestId("add-a"));
    expect(onSwitch).toHaveBeenCalledWith(providerA);
  });
  it.each([{ ctrlKey: true }, { metaKey: true }])(
    "keeps search in the list flow and restores cards on close (%o)",
    async (shortcut) => {
      const alpha = createProvider({ id: "alpha", name: "Alpha Labs" });
      const beta = createProvider({ id: "beta", name: "Beta Works" });
      const onEdit = vi.fn();
      useDragSortMock.mockReturnValue({
        sortedProviders: [alpha, beta],
        sensors: [],
        handleDragEnd: vi.fn(),
      });
      const { container } = renderWithQueryClient(
        <ProviderList
          {...baseProps}
          providers={{ alpha, beta }}
          appId="codex"
          switchMode={switchMode()}
          onEdit={onEdit}
        />,
      );
      fireEvent.keyDown(window, { key: "f", ...shortcut });
      const input = screen.getByRole("textbox", { name: "Search providers" });
      // A body portal with fixed positioning covers the first card's actions.
      // Keep the panel before the cards in the list's normal document flow.
      expect(container).toContainElement(input);
      const list = container.firstElementChild!;
      const panel = list.firstElementChild!;
      expect(panel).toContainElement(input);
      expect(panel).not.toHaveClass("fixed");
      expect(panel).not.toHaveClass("absolute");
      expect(panel.nextElementSibling).toContainElement(
        screen.getByTestId("provider-card-alpha"),
      );
      await waitFor(() => expect(input).toHaveFocus());
      fireEvent.change(input, { target: { value: "beta" } });
      expect(
        screen.queryByTestId("provider-card-alpha"),
      ).not.toBeInTheDocument();
      fireEvent.click(screen.getByTestId("edit-beta"));
      expect(onEdit).toHaveBeenCalledWith(beta);
      fireEvent.keyDown(window, { key: "Escape" });
      await waitFor(() => expect(input).not.toBeInTheDocument());
      expect(screen.getByTestId("provider-card-alpha")).toBeInTheDocument();
      fireEvent.keyDown(window, { key: "f", ...shortcut });
      const reopened = screen.getByRole("textbox", {
        name: "Search providers",
      });
      expect(reopened).toHaveValue("beta");
      fireEvent.click(screen.getByRole("button", { name: "Clear" }));
      expect(screen.getByTestId("provider-card-alpha")).toBeInTheDocument();
      fireEvent.click(
        screen.getByRole("button", { name: "Close provider search" }),
      );
      await waitFor(() => expect(reopened).not.toBeInTheDocument());
    },
  );
  it.each([
    { shortcut: { ctrlKey: true }, controlled: false },
    { shortcut: { metaKey: true }, controlled: false },
    { shortcut: { ctrlKey: true }, controlled: true },
    { shortcut: { metaKey: true }, controlled: true },
  ])(
    "refocuses an already open search panel (%o)",
    async ({ shortcut, controlled }) => {
      const providers = Array.from({ length: 30 }, (_, index) =>
        createProvider({ id: `provider-${index}`, name: `Provider ${index}` }),
      );
      useDragSortMock.mockReturnValue({
        sortedProviders: providers,
        sensors: [],
        handleDragEnd: vi.fn(),
      });
      function SearchList() {
        const [open, setOpen] = useState(false);
        return (
          <div
            data-testid="scroll-container"
            style={{ height: 200, overflowY: "auto" }}
          >
            <ProviderList
              {...baseProps}
              providers={Object.fromEntries(
                providers.map((provider) => [provider.id, provider]),
              )}
              appId="claude"
              {...(controlled
                ? { searchOpen: open, onSearchOpenChange: setOpen }
                : {})}
            />
          </div>
        );
      }
      renderWithQueryClient(<SearchList />);
      fireEvent.keyDown(window, { key: "f", ...shortcut });
      const input = screen.getByRole("textbox", {
        name: "Search providers",
      }) as HTMLInputElement;
      await waitFor(() => expect(input).toHaveFocus());
      fireEvent.change(input, { target: { value: "Provider" } });
      const scrollIntoView = vi.fn();
      input.scrollIntoView = scrollIntoView;
      // jsdom has no layout: model scrolling away and assert the browser scroll request.
      const scrollContainer = screen.getByTestId("scroll-container");
      for (let invocation = 1; invocation <= 2; invocation++) {
        scrollContainer.scrollTop = 1000;
        screen.getByTestId("edit-provider-29").focus();
        expect(input).not.toHaveFocus();
        const event = new KeyboardEvent("keydown", {
          key: "f",
          ...shortcut,
          cancelable: true,
        });
        fireEvent(window, event);
        expect(event.defaultPrevented).toBe(true);
        await waitFor(() => expect(input).toHaveFocus());
        expect(input.selectionStart).toBe(0);
        expect(input.selectionEnd).toBe("Provider".length);
        expect(scrollIntoView).toHaveBeenCalledTimes(invocation);
        expect(scrollIntoView).toHaveBeenLastCalledWith({ block: "nearest" });
      }
    },
  );
  it.each(["input", "textarea", "select", "contenteditable"])(
    "does not steal the search shortcut from another %s",
    async (targetType) => {
      const provider = createProvider();
      useDragSortMock.mockReturnValue({
        sortedProviders: [provider],
        sensors: [],
        handleDragEnd: vi.fn(),
      });
      renderWithQueryClient(
        <ProviderList
          {...baseProps}
          providers={{ [provider.id]: provider }}
          appId="claude"
        />,
      );
      fireEvent.keyDown(window, { key: "f", ctrlKey: true });
      const input = screen.getByRole("textbox", { name: "Search providers" });
      await waitFor(() => expect(input).toHaveFocus());
      const scrollIntoView = vi.fn();
      input.scrollIntoView = scrollIntoView;
      const target = document.createElement(
        targetType === "contenteditable" ? "div" : targetType,
      );
      if (targetType === "contenteditable") {
        target.tabIndex = 0;
        // jsdom does not implement isContentEditable.
        Object.defineProperty(target, "isContentEditable", { value: true });
      }
      document.body.appendChild(target);
      try {
        target.focus();
        const event = new KeyboardEvent("keydown", {
          key: "f",
          ctrlKey: true,
          cancelable: true,
        });
        fireEvent(window, event);
        expect(event.defaultPrevented).toBe(false);
        expect(target).toHaveFocus();
        expect(scrollIntoView).not.toHaveBeenCalled();
      } finally {
        target.remove();
      }
    },
  );
  it("renders the search field into the mode row slot instead of the list", async () => {
    const alpha = createProvider({ id: "alpha", name: "Alpha Labs" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [alpha],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    const slot = document.createElement("div");
    document.body.appendChild(slot);
    try {
      const { container } = renderWithQueryClient(
        <ProviderList
          {...baseProps}
          providers={{ alpha }}
          appId="codex"
          switchMode={switchMode()}
          searchSlot={slot}
        />,
      );
      fireEvent.keyDown(window, { key: "f", ctrlKey: true });
      const input = screen.getByRole("textbox", { name: "Search providers" });
      expect(slot).toContainElement(input);
      expect(container).not.toContainElement(input);
      await waitFor(() => expect(input).toHaveFocus());
    } finally {
      slot.remove();
    }
  });
  it("renders nothing for a mode row slot that is not mounted yet", () => {
    const alpha = createProvider({ id: "alpha", name: "Alpha Labs" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [alpha],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{ alpha }}
        appId="codex"
        switchMode={switchMode()}
        searchSlot={null}
        searchOpen
      />,
    );
    expect(
      screen.queryByRole("textbox", { name: "Search providers" }),
    ).not.toBeInTheDocument();
    expect(screen.getByTestId("provider-card-alpha")).toBeInTheDocument();
  });
  it("puts the search field beside the first section title and keeps it while no provider matches", async () => {
    const live = createProvider({ id: "live", name: "Live Labs" });
    const other = createProvider({ id: "other", name: "Other Works" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [live, other],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    server.use(
      http.post(`${TAURI_ENDPOINT}/get_opencode_live_provider_ids`, () =>
        HttpResponse.json(["live"]),
      ),
    );
    renderWithQueryClient(
      <ProviderList
        {...baseProps}
        providers={{ live, other }}
        appId="opencode"
      />,
    );
    const heading = await screen.findByRole("heading", {
      name: "providerCard.section.added",
    });
    fireEvent.keyDown(window, { key: "f", ctrlKey: true });
    const input = screen.getByRole("textbox", { name: "Search providers" });
    // 同一行：标题和搜索框共用列表的第一行，第一个分组里不再重复画标题
    const header = input.closest("div.min-h-8");
    expect(header).toContainElement(heading);
    expect(
      screen.getAllByRole("heading", { name: "providerCard.section.added" }),
    ).toHaveLength(1);
    await waitFor(() => expect(input).toHaveFocus());
    fireEvent.change(input, { target: { value: "nothing-matches" } });
    expect(screen.getByText("No providers match your search.")).toBeVisible();
    // 结果清空时输入框还是同一个节点，焦点不丢
    expect(screen.getByRole("textbox", { name: "Search providers" })).toBe(
      input,
    );
    expect(input).toHaveFocus();
  });
  it("filters providers with the search input", () => {
    const providerAlpha = createProvider({ id: "alpha", name: "Alpha Labs" });
    const providerBeta = createProvider({ id: "beta", name: "Beta Works" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [providerAlpha, providerBeta],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    renderWithQueryClient(
      <ProviderList
        providers={{ alpha: providerAlpha, beta: providerBeta }}
        currentProviderId=""
        appId="claude"
        onSwitch={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
        onOpenWebsite={vi.fn()}
      />,
    );
    fireEvent.keyDown(window, { key: "f", metaKey: true });
    const searchInput = screen.getByPlaceholderText(
      "Search name, notes, or URL...",
    );
    // Initially both providers are rendered
    expect(screen.getByTestId("provider-card-alpha")).toBeInTheDocument();
    expect(screen.getByTestId("provider-card-beta")).toBeInTheDocument();
    fireEvent.change(searchInput, { target: { value: "beta" } });
    expect(screen.queryByTestId("provider-card-alpha")).not.toBeInTheDocument();
    expect(screen.getByTestId("provider-card-beta")).toBeInTheDocument();
    fireEvent.change(searchInput, { target: { value: "gamma" } });
    expect(screen.queryByTestId("provider-card-alpha")).not.toBeInTheDocument();
    expect(screen.queryByTestId("provider-card-beta")).not.toBeInTheDocument();
    expect(
      screen.getByText("No providers match your search."),
    ).toBeInTheDocument();
  });
  it("matches the API address and stops filtering once the panel closes", () => {
    const relay = createProvider({
      id: "relay",
      name: "My Relay",
      settingsConfig: {
        env: { ANTHROPIC_BASE_URL: "https://api.relay-example.com" },
      },
    });
    const other = createProvider({ id: "other", name: "Other" });
    useDragSortMock.mockReturnValue({
      sortedProviders: [relay, other],
      sensors: [],
      handleDragEnd: vi.fn(),
    });
    const props = {
      providers: { relay, other },
      currentProviderId: "",
      appId: "claude" as const,
      onSwitch: vi.fn(),
      onEdit: vi.fn(),
      onDelete: vi.fn(),
      onDuplicate: vi.fn(),
      onOpenWebsite: vi.fn(),
    };
    const onSearchOpenChange = vi.fn();
    const queryClient = new QueryClient({
      defaultOptions: { queries: { retry: false } },
    });
    const ui = (searchOpen: boolean) => (
      <QueryClientProvider client={queryClient}>
        <ProviderList
          {...props}
          searchOpen={searchOpen}
          onSearchOpenChange={onSearchOpenChange}
        />
      </QueryClientProvider>
    );
    const { rerender } = render(ui(true));
    fireEvent.change(
      screen.getByPlaceholderText("Search name, notes, or URL..."),
      { target: { value: "relay-example" } },
    );
    expect(screen.getByTestId("provider-card-relay")).toBeInTheDocument();
    expect(screen.queryByTestId("provider-card-other")).not.toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(onSearchOpenChange).toHaveBeenCalledWith(false);
    rerender(ui(false));
    expect(screen.getByTestId("provider-card-other")).toBeInTheDocument();
  });
  it("shows the signed-in DeepSeek account as an official card under Added", async () => {
    dshAccountStatusMock.mockReturnValue({
      data: { signedIn: true, expired: false, profile: { name: "Jason" } },
    });
    renderWithQueryClient(
      <ProviderList
        providers={{}}
        currentProviderId=""
        appId="dsh"
        onSwitch={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
        onOpenWebsite={vi.fn()}
        onCreate={vi.fn()}
      />,
    );
    expect(await screen.findByTestId("dsh-account-card")).toHaveTextContent(
      "Jason",
    );
    expect(screen.getByText("providerCard.section.added")).toBeInTheDocument();
    expect(
      screen.queryByText("providerCard.section.addedEmpty"),
    ).not.toBeInTheDocument();
    expect(
      screen.queryByRole("button", { name: "provider.addProvider" }),
    ).not.toBeInTheDocument();
  });
  it("keeps the empty state and no account card while signed out of DeepSeek", async () => {
    dshAccountStatusMock.mockReturnValue({
      data: { signedIn: false, expired: false, profile: null },
    });
    renderWithQueryClient(
      <ProviderList
        providers={{}}
        currentProviderId=""
        appId="dsh"
        onSwitch={vi.fn()}
        onEdit={vi.fn()}
        onDelete={vi.fn()}
        onDuplicate={vi.fn()}
        onOpenWebsite={vi.fn()}
        onCreate={vi.fn()}
      />,
    );
    expect(
      await screen.findByRole("button", { name: "provider.addProvider" }),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("dsh-account-card")).not.toBeInTheDocument();
  });
});
