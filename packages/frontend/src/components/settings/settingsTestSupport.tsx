import { act, type ReactElement } from "react";
import { createRoot, type Root } from "react-dom/client";
import type { AgentSettingsRecord } from "@openaide/app-shell-contracts";
import { SettingsView, type SettingsViewProps } from "./SettingsView";

/**
 * DOM helpers for Settings tests. They find controls the way a user does, by
 * name or visible text, so a test survives markup and class changes.
 */

let mounted: { container: HTMLElement; root: Root } | undefined;

export async function mount(element: ReactElement) {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  if (!mounted) {
    const container = document.createElement("div");
    document.body.append(container);
    mounted = { container, root: createRoot(container) };
  }
  await act(async () => mounted!.root.render(element));
}

export function unmount() {
  if (mounted) act(() => mounted!.root.unmount());
  mounted = undefined;
  document.body.replaceChildren();
}

export function settingsProps(overrides: Partial<SettingsViewProps> = {}): SettingsViewProps {
  return {
    onAuthenticate: () => undefined,
    onCreateCustomAgent: () => undefined,
    onDeleteCustomAgent: () => undefined,
    onRefresh: () => undefined,
    onReplaceCustomAgent: () => undefined,
    onSelectTab: () => undefined,
    onSetAcpTrace: () => undefined,
    onSetAgentEnabled: async () => ({ kind: "applied" }),
    onSetComposerSubmitShortcut: () => undefined,
    onUpdateCustomAgentMetadata: async () => ({ kind: "applied" }),
    preferences: { composer_submit_shortcut: "mod_enter" },
    state: { activeTab: "general", loading: false },
    ...overrides,
  };
}

export function mountSettings(overrides: Partial<SettingsViewProps> = {}) {
  return mount(<SettingsView {...settingsProps(overrides)} />);
}

export function pageText() {
  return document.body.textContent ?? "";
}

/** A button by its accessible name: `aria-label` when present, otherwise its text. */
export function button(name: string, index = 0) {
  const match = buttons(name)[index];
  if (!match) throw new Error(`Button not found: ${name}`);
  return match;
}

export function buttons(name: string) {
  return [...document.querySelectorAll<HTMLElement>("button, a")]
    .filter((candidate) => (candidate.getAttribute("aria-label") ?? candidate.textContent?.trim()) === name);
}

export function field(label: string) {
  const match = document.querySelector<HTMLInputElement | HTMLTextAreaElement>(`[aria-label="${label}"]`);
  if (!match) throw new Error(`Field not found: ${label}`);
  return match;
}

export function dialog(label: string) {
  return document.querySelector<HTMLElement>(`[role="dialog"][aria-label="${label}"]`);
}

export async function click(name: string, index = 0) {
  await act(async () => button(name, index).click());
}

export async function type(label: string, value: string) {
  const input = field(label);
  const prototype = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
  await act(async () => {
    Object.getOwnPropertyDescriptor(prototype, "value")!.set!.call(input, value);
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
}

/** Lets promises started by the last interaction resolve and re-render. */
export async function settle() {
  await act(async () => {
    await Promise.resolve();
    await Promise.resolve();
    await Promise.resolve();
  });
}

export function builtInAgent(overrides: Partial<AgentSettingsRecord> = {}): AgentSettingsRecord {
  return {
    id: "codex",
    label: "Codex",
    enabled: true,
    scope: "global",
    source_kind: "built_in",
    icon: "openai",
    transport: "stdio",
    status: "connected",
    launch_label: "codex",
    command_line: undefined,
    description: "OpenAI coding agent.",
    capabilities: [],
    auth_methods: [],
    ...overrides,
  };
}

export function customAgent(overrides: Partial<AgentSettingsRecord> = {}): AgentSettingsRecord {
  return {
    id: "custom-1",
    label: "Local Agent",
    enabled: true,
    scope: "global",
    source_kind: "custom",
    icon: "terminal",
    transport: "stdio",
    status: "connected",
    launch_label: "local-agent",
    command_line: "local-agent --stdio",
    env: [],
    description: "Custom ACP stdio Agent",
    capabilities: [],
    auth_methods: [],
    ...overrides,
  };
}
