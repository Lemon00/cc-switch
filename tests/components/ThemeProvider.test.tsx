import { fireEvent, render, screen } from "@testing-library/react";
import { ThemeProvider, useTheme } from "@/components/theme-provider";

vi.mock("@tauri-apps/api/core", () => ({
  invoke: vi.fn().mockResolvedValue(undefined),
}));

function ThemeControl() {
  const { theme, setTheme } = useTheme();
  return <button onClick={() => setTheme("dark")}>{theme}</button>;
}

it("keeps the interface and theme controls usable when storage is blocked", () => {
  vi.stubGlobal(
    "matchMedia",
    vi.fn(() => ({
      matches: false,
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    })),
  );
  vi.spyOn(Storage.prototype, "getItem").mockImplementation(() => {
    throw new DOMException("blocked", "SecurityError");
  });
  vi.spyOn(Storage.prototype, "setItem").mockImplementation(() => {
    throw new DOMException("full", "QuotaExceededError");
  });
  try {
    render(
      <ThemeProvider defaultTheme="light">
        <ThemeControl />
      </ThemeProvider>,
    );
    fireEvent.click(screen.getByRole("button", { name: "light" }));
    expect(screen.getByRole("button", { name: "dark" })).toBeInTheDocument();
    expect(document.documentElement).toHaveClass("dark");
  } finally {
    vi.restoreAllMocks();
    vi.unstubAllGlobals();
    document.documentElement.classList.remove(
      "dark",
      "light",
      "theme-switching",
    );
  }
});
