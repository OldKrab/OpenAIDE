import { expect, it, vi } from "vitest";
import { SETTINGS_GET_RUNTIME } from "@openaide/app-server-client";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";
import { refreshSettingsProjectionsThroughBackend, type SettingsProjectionIntentContext } from "./settingsProjectionIntents";

function context(runtime: () => Promise<unknown>) {
  const actions: AppAction[] = [];
  const request = vi.fn(async (method: string) => {
    if (method === SETTINGS_GET_RUNTIME) return runtime();
    return { generatedAt: "now", agents: [], servers: [], skills: [], availability: "available" };
  });
  const intentContext: SettingsProjectionIntentContext = {
    backendConnection: { request } as unknown as SettingsProjectionIntentContext["backendConnection"],
    currentAgentId: "codex",
    dispatch: (action) => actions.push(action),
    setAgents: () => undefined,
    state: {} as AppState,
  };
  return { actions, intentContext };
}

it("reads the trace setting with the other Settings projections", async () => {
  const { actions, intentContext } = context(async () => ({ developer: { acpTrace: { enabled: true, directory: "traces" } } }));

  await expect(refreshSettingsProjectionsThroughBackend(intentContext)).resolves.toBe(true);

  expect(actions).toContainEqual({
    type: "settings:runtimeSettings",
    settings: { developer: { acp_trace: { enabled: true, directory: "traces" } } },
  });
});

it("still shows Agents, MCP servers, and Skills when the trace setting cannot be read", async () => {
  const { actions, intentContext } = context(async () => { throw new Error("unsupported"); });

  await expect(refreshSettingsProjectionsThroughBackend(intentContext)).resolves.toBe(true);

  const types = actions.map((action) => action.type);
  expect(types).toEqual(expect.arrayContaining(["settings:agentDetailsResult", "settings:mcpServersResult", "settings:skillsResult"]));
  expect(types).not.toContain("settings:runtimeSettings");
});

it("reads nothing without an App Server connection", async () => {
  const { actions, intentContext } = context(async () => ({}));

  await expect(refreshSettingsProjectionsThroughBackend({ ...intentContext, backendConnection: undefined })).resolves.toBe(false);
  expect(actions).toEqual([]);
});
