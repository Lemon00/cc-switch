import type { ReactElement } from "react";
import { QueryClientProvider } from "@tanstack/react-query";
import { render as rtlRender, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { DshProviderForm } from "@/components/providers/forms/DshProviderForm";
import { createTestQueryClient } from "../utils/testQueryClient";
vi.mock("@/components/JsonEditor", () => ({
  default: ({
    id,
    value,
    onChange,
  }: {
    id: string;
    value: string;
    onChange: (value: string) => void;
  }) => (
    <textarea
      id={id}
      value={value}
      onChange={(event) => onChange(event.target.value)}
    />
  ),
}));
const render = (ui: ReactElement) =>
  rtlRender(
    <QueryClientProvider client={createTestQueryClient()}>
      {ui}
    </QueryClientProvider>,
  );
const original = {
  kind: "custom",
  api: "anthropic-messages",
  enabled: true,
  options: {
    baseURL: "https://api.example.com/anthropic",
    apiKey: "local-test-key",
    headers: { "X-Test": "keep" },
  },
  models: {
    model: {
      name: "Model",
      limit: { context: 200000 },
      thinking: { effort: "high" },
    },
  },
  futureSetting: { keep: true },
};
describe("DshProviderForm", () => {
  it("keeps the JSON editor for DeepSeek Harness", () => {
    render(
      <DshProviderForm
        appId="dsh"
        providerId="existing"
        initialData={{ name: "Existing", settingsConfig: original }}
        submitLabel="Save"
        onSubmit={vi.fn()}
        onCancel={() => {}}
      />,
    );
    expect(
      JSON.parse(
        (screen.getByLabelText("provider.configJson") as HTMLTextAreaElement)
          .value,
      ),
    ).toEqual(original);
  });
});
