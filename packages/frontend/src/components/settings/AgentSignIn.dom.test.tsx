// @vitest-environment jsdom

import { act, useState } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { AgentSignIn } from "./AgentSignIn";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.replaceChildren();
});

it("keeps the API key form usable after cancelling another sign-in method", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<AuthHarness />));

  await act(async () => button("Cancel sign-in").click());
  await act(async () => button("API key").click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="API key"]')!;

  await act(async () => {
    const setValue = Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!;
    for (const value of ["d", "du", "dum", "dumm", "dummy"]) {
      setValue.call(input, value);
      input.dispatchEvent(new Event("input", { bubbles: true }));
    }
  });

  expect(document.querySelector<HTMLInputElement>('input[aria-label="API key"]')?.value).toBe("dummy");
  expect(container.textContent).toContain("Enter your API key");
});

it("submits a complete API key form when the user presses Enter", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const onAuthenticate = vi.fn();
  await act(async () => root.render(
    <AgentSignIn
      agent={{ ...agentRecord, status: "connected", sign_in: undefined }}
      onAuthenticate={onAuthenticate}
    />,
  ));

  await act(async () => button("Manage").click());
  await act(async () => button("API key").click());
  const input = document.querySelector<HTMLInputElement>('input[aria-label="API key"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLInputElement.prototype, "value")!.set!.call(input, "dummy");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => input.dispatchEvent(new KeyboardEvent("keydown", { bubbles: true, key: "Enter" })));

  expect(onAuthenticate).toHaveBeenCalledWith("codex", "api-key", { API_KEY: "dummy" });
});

it("offers sign out only for a connected Agent that advertises logout", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  const onLogout = vi.fn(async () => true);
  await act(async () => root.render(
    <AgentSignIn
      agent={{ ...agentRecord, status: "connected", sign_in: undefined, logout_supported: true }}
      onAuthenticate={() => undefined}
      onLogout={onLogout}
    />,
  ));

  expect(container.textContent).toContain("Authentication");
  await act(async () => button("Sign out").click());
  expect(onLogout).toHaveBeenCalledWith("codex");
});

it("disables sign out while the Agent has a running Task", async () => {
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(
    <AgentSignIn
      agent={{
        ...agentRecord,
        status: "connected",
        sign_in: undefined,
        logout_supported: true,
        logout_blocked_by_running_task: true,
      }}
      onAuthenticate={() => undefined}
      onLogout={() => undefined}
    />,
  ));

  expect(button("Sign out").disabled).toBe(true);
  expect(button("Sign out").title).toBe("Stop running Tasks before signing out.");
});

function AuthHarness() {
  const [agent, setAgent] = useState(agentRecord);
  return (
    <AgentSignIn
      agent={agent}
      onAuthenticate={() => undefined}
      onCancel={async () => setAgent((current) => ({ ...current, sign_in: undefined, status: "connected" }))}
    />
  );
}

function button(label: string) {
  return [...document.querySelectorAll("button")].find((candidate) => candidate.textContent === label)!;
}

const agentRecord: AgentSettingsRecord = {
  id: "codex",
  label: "Codex",
  enabled: true,
  scope: "global",
  source_kind: "built_in",
  icon: "openai",
  transport: "stdio",
  status: "authenticating",
  launch_label: "Managed by OpenAIDE",
  description: "Codex",
  capabilities: [],
  auth_methods: [
    {
      id: "api-key",
      label: "API key",
      kind: "env_var",
      variables: [{ name: "API_KEY", label: "API key", secret: true, optional: false }],
    },
    { id: "device", label: "Device code", kind: "agent" },
  ],
  sign_in: {
    method_id: "device",
    phase: "awaiting_user",
    url: "https://example.com/device",
  },
};
