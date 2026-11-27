import { render, screen, fireEvent } from "@testing-library/react";
import { describe, it, expect, vi, beforeEach } from "vitest";
import "@testing-library/jest-dom";
import userEvent from "@testing-library/user-event";
import { GeneralSection } from "@/components/settings/sections/GeneralSection";
import type { SettingsFormState } from "@/hooks/useSettings";
vi.mock("react-i18next", () => ({
  useTranslation: () => ({ t: (key: string) => key }),
}));
vi.mock("@/components/theme-provider", () => ({
  useTheme: () => ({ theme: "system", setTheme: vi.fn() }),
}));
vi.mock("@/components/settings/TerminalSettings", () => ({
  TerminalSelect: () => null,
}));
const platform = vi.hoisted(() => ({ linux: false }));
vi.mock("@/lib/platform", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@/lib/platform")>()),
  isLinux: () => platform.linux,
}));
const baseSettings = {
  showInTray: true,
  minimizeToTrayOnClose: true,
  language: "zh",
} as SettingsFormState;
function renderSection(settings: Partial<SettingsFormState> = {}) {
  const onAutoSave = vi.fn().mockResolvedValue(true);
  render(
    <GeneralSection
      settings={{ ...baseSettings, ...settings }}
      onAutoSave={onAutoSave}
      onOpenApps={vi.fn()}
    />,
  );
  return onAutoSave;
}
describe("GeneralSection tray style", () => {
  it.each(["zh", "en"] as const)(
    "saves a language change from %s",
    async (language) => {
      const user = userEvent.setup();
      const onAutoSave = renderSection({ language });
      await user.click(
        screen.getByRole("combobox", { name: "settings.language" }),
      );
      const target = language === "zh" ? "en" : "zh";
      await user.click(
        await screen.findByRole("option", {
          name: target === "en" ? "English" : "简体中文",
        }),
      );
      expect(onAutoSave).toHaveBeenCalledWith({ language: target });
    },
  );
  beforeEach(() => {
    platform.linux = false;
    Element.prototype.scrollIntoView = vi.fn();
  });
  it("defaults to the panel and saves the native menu when Menu is picked", () => {
    const onAutoSave = renderSection();
    const group = screen.getByRole("group", {
      name: "settings.general.trayStyle",
    });
    expect(group).toBeInTheDocument();
    expect(
      screen.getByRole("button", { name: "settings.general.trayStylePanel" }),
    ).toHaveAttribute("aria-pressed", "true");
    fireEvent.click(
      screen.getByRole("button", { name: "settings.general.trayStyleMenu" }),
    );
    expect(onAutoSave).toHaveBeenCalledWith({ nativeTrayMenu: true });
  });
  it("switches back to the panel", () => {
    const onAutoSave = renderSection({ nativeTrayMenu: true });
    fireEvent.click(
      screen.getByRole("button", { name: "settings.general.trayStylePanel" }),
    );
    expect(onAutoSave).toHaveBeenCalledWith({ nativeTrayMenu: false });
  });
  it("is hidden on Linux, which only has the native menu", () => {
    platform.linux = true;
    renderSection();
    expect(
      screen.queryByRole("group", { name: "settings.general.trayStyle" }),
    ).not.toBeInTheDocument();
  });
});
