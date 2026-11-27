import { act, fireEvent, render, screen, within } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { ReactNode } from "react";
import { ProfileSwitcher } from "@/components/profiles/ProfileSwitcher";
import { ProfileManageDialog } from "@/components/profiles/ProfileManageDialog";
const mocks = vi.hoisted(() => ({
  create: vi.fn(),
  update: vi.fn(),
  apply: vi.fn(),
  clear: vi.fn(),
  remove: vi.fn(),
}));
vi.mock("@/lib/query/profiles", () => ({
  useProfilesQuery: () => ({
    data: {
      profiles: ["First", "Second"].map((name) => ({
        id: name,
        name,
        payload: { providers: {}, mcp: {}, skills: {}, prompts: {} },
      })),
      currentIds: {},
    },
  }),
  useCreateProfileMutation: () => ({ mutate: mocks.create, isPending: false }),
  useUpdateProfileMutation: () => ({ mutate: mocks.update, isPending: false }),
  useApplyProfileMutation: () => ({ mutate: mocks.apply }),
  useClearProfileMutation: () => ({ mutate: mocks.clear }),
  useDeleteProfileMutation: () => ({ mutate: mocks.remove }),
}));
vi.mock("@/components/ui/popover", () => ({
  Popover: ({ children }: { children: ReactNode }) => <div>{children}</div>,
  PopoverTrigger: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
  PopoverContent: ({ children }: { children: ReactNode }) => (
    <div>{children}</div>
  ),
}));
vi.mock("@/components/ui/command", () => ({
  Command: ({ children }: any) => <div>{children}</div>,
  CommandList: ({ children }: any) => <div>{children}</div>,
  CommandGroup: ({ children }: any) => <div>{children}</div>,
  CommandEmpty: () => null,
  CommandInput: () => null,
  CommandItem: ({ children, onSelect }: any) => (
    <button onClick={onSelect}>{children}</button>
  ),
}));
vi.mock("@/components/ui/hover-tip", () => ({
  HoverTip: ({ children }: any) => children,
}));
describe("Profile dialog sessions", () => {
  beforeEach(() => vi.clearAllMocks());
  it("submits a new profile only once when Enter is repeated and permits retry after failure", () => {
    render(<ProfileSwitcher activeApp="claude" />);
    fireEvent.click(screen.getByText("profiles.createFromCurrent"));
    const input = screen.getByPlaceholderText("profiles.namePlaceholder");
    fireEvent.change(input, { target: { value: "Project" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(mocks.create).toHaveBeenCalledTimes(1);
    act(() => mocks.create.mock.calls[0][1].onSettled());
    fireEvent.keyDown(input, { key: "Enter" });
    expect(mocks.create).toHaveBeenCalledTimes(2);
  });
  it("keeps a newly opened creation draft when an older save succeeds", () => {
    render(<ProfileSwitcher activeApp="claude" />);
    fireEvent.click(screen.getByText("profiles.createFromCurrent"));
    fireEvent.change(screen.getByPlaceholderText("profiles.namePlaceholder"), {
      target: { value: "Old" },
    });
    fireEvent.click(screen.getByRole("button", { name: "common.confirm" }));
    const callbacks = mocks.create.mock.calls[0][1];
    fireEvent.click(screen.getByRole("button", { name: "common.cancel" }));
    fireEvent.click(screen.getByText("profiles.createFromCurrent"));
    fireEvent.change(screen.getByPlaceholderText("profiles.namePlaceholder"), {
      target: { value: "New draft" },
    });
    act(() => callbacks.onSuccess());
    expect(screen.getByPlaceholderText("profiles.namePlaceholder")).toHaveValue(
      "New draft",
    );
  });
  it("closes the old scope's draft when the active app changes", () => {
    const { rerender } = render(<ProfileSwitcher activeApp="claude" />);
    fireEvent.click(screen.getByText("profiles.createFromCurrent"));
    rerender(<ProfileSwitcher activeApp="codex" />);
    expect(
      screen.queryByPlaceholderText("profiles.namePlaceholder"),
    ).not.toBeInTheDocument();
  });
  it("does not cancel another rename when an earlier save finishes", () => {
    render(<ProfileManageDialog isOpen onClose={vi.fn()} />);
    fireEvent.click(
      screen.getAllByRole("button", { name: "profiles.rename" })[0],
    );
    let input = within(screen.getByRole("dialog")).getByRole("textbox");
    fireEvent.change(input, { target: { value: "First updated" } });
    fireEvent.keyDown(input, { key: "Enter" });
    fireEvent.keyDown(input, { key: "Enter" });
    expect(mocks.update).toHaveBeenCalledOnce();
    const callbacks = mocks.update.mock.calls[0][1];
    fireEvent.click(screen.getByRole("button", { name: "profiles.rename" }));
    input = within(screen.getByRole("dialog")).getByRole("textbox");
    fireEvent.change(input, { target: { value: "Second draft" } });
    act(() => callbacks.onSuccess());
    expect(input).toHaveValue("Second draft");
  });
});
