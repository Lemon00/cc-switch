import {
  act,
  cleanup,
  fireEvent,
  render,
  screen,
  within,
} from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import EndpointSpeedTest from "@/components/providers/forms/EndpointSpeedTest";

const api = vi.hoisted(() => ({
  testApiEndpoints: vi.fn(),
  getCustomEndpoints: vi.fn(),
}));
vi.mock("@/lib/api/vscode", () => ({ vscodeApi: api }));
vi.mock("react-i18next", () => {
  const t = (key: string) => key;
  return { useTranslation: () => ({ t }) };
});
vi.mock("@/components/common/FullScreenPanel", () => ({
  FullScreenPanel: ({ children, footer }: any) => (
    <div>
      {children}
      {footer}
    </div>
  ),
}));
const first = "https://first.example";
const fastest = "https://fastest.example";
const props = () => ({
  appId: "claude" as const,
  value: first,
  onChange: vi.fn(),
  initialEndpoints: [{ url: first }, { url: fastest }],
  onClose: vi.fn(),
  autoSelect: true,
  onAutoSelectChange: vi.fn(),
});
let finish!: (results: unknown[]) => void;
beforeEach(() => {
  api.getCustomEndpoints.mockReset().mockResolvedValue([]);
  api.testApiEndpoints.mockReset().mockImplementation(
    () =>
      new Promise((resolve) => {
        finish = resolve;
      }),
  );
});
afterEach(cleanup);
const complete = async () =>
  act(async () =>
    finish([
      { url: first, latency: 200 },
      { url: fastest, latency: 10 },
    ]),
  );
describe("endpoint speed test ownership", () => {
  it.each(["unmount", "hide", "provider"] as const)(
    "ignores a result after %s",
    async (change) => {
      const input = props();
      const { unmount, rerender } = render(<EndpointSpeedTest {...input} />);
      fireEvent.click(screen.getByText("endpointTest.testSpeed"));
      if (change === "unmount") unmount();
      else if (change === "hide")
        rerender(<EndpointSpeedTest {...input} visible={false} />);
      else rerender(<EndpointSpeedTest {...input} providerId="another" />);
      await complete();
      expect(input.onChange).not.toHaveBeenCalled();
    },
  );
  it("does not auto-select an endpoint removed during the test", async () => {
    const input = props();
    render(<EndpointSpeedTest {...input} />);
    fireEvent.click(screen.getByText("endpointTest.testSpeed"));
    const row =
      screen.getByText(fastest).parentElement!.parentElement!.parentElement!;
    fireEvent.click(within(row).getByRole("button"));
    await complete();
    expect(input.onChange).not.toHaveBeenCalled();
  });
  it("honors turning off automatic selection while the test is pending", async () => {
    const input = props();
    const { rerender } = render(<EndpointSpeedTest {...input} />);
    fireEvent.click(screen.getByText("endpointTest.testSpeed"));
    rerender(<EndpointSpeedTest {...input} autoSelect={false} />);
    await complete();
    expect(input.onChange).not.toHaveBeenCalled();
  });
  it("selects the fastest remaining endpoint when the user has made no newer choice", async () => {
    const input = props();
    render(<EndpointSpeedTest {...input} />);
    fireEvent.click(screen.getByText("endpointTest.testSpeed"));
    await complete();
    expect(input.onChange).toHaveBeenCalledWith(fastest);
  });
  it("keeps a newer manual selection", async () => {
    const input = props();
    const { rerender } = render(<EndpointSpeedTest {...input} />);
    fireEvent.click(screen.getByText("endpointTest.testSpeed"));
    rerender(<EndpointSpeedTest {...input} value="https://manual.example" />);
    await complete();
    expect(input.onChange).not.toHaveBeenCalled();
  });
  it("starts only one request on consecutive clicks", async () => {
    const input = props();
    render(<EndpointSpeedTest {...input} />);
    const start = screen.getByText("endpointTest.testSpeed");
    act(() => {
      fireEvent.click(start);
      fireEvent.click(start);
    });
    expect(api.testApiEndpoints).toHaveBeenCalledTimes(1);
    await complete();
  });
  it("does not reload saved endpoints when the parent updates candidates", async () => {
    const input = { ...props(), providerId: "saved" };
    const { rerender } = render(<EndpointSpeedTest {...input} />);
    await act(async () => {});
    rerender(
      <EndpointSpeedTest
        {...input}
        initialEndpoints={[...input.initialEndpoints]}
      />,
    );
    await act(async () => {});
    expect(api.getCustomEndpoints).toHaveBeenCalledTimes(1);
  });
});
