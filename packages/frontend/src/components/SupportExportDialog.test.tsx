// @vitest-environment jsdom

import { act } from "react";
import type { ReactNode } from "react";
import { createRoot } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import {
  DIAGNOSTICS_CREATE_SUPPORT_EXPORT,
  DIAGNOSTICS_LIST_SUPPORT_EXPORT,
} from "@openaide/app-server-client";
import { installFrontendShell, type FrontendShell } from "../services/frontendShell";
import { SupportExportButton } from "./SupportExportDialog";

vi.mock("./Popup", () => ({
  PopupDialog: ({ children, label, open }: { children: ReactNode; label: string; open: boolean }) => open
    ? <div aria-label={label} role="dialog">{children}</div>
    : null,
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  document.body.replaceChildren();
});

it("preselects the current Task's sensitive sources and saves before opening the bug form", async () => {
  const save = vi.fn(async () => undefined);
  const openExternal = vi.fn();
  const request = vi.fn(async (method: string) => {
    if (method === DIAGNOSTICS_LIST_SUPPORT_EXPORT) {
      return {
        acpTraceEnabled: true,
        sessions: [{
          acpTraceCount: 2,
          active: true,
          agentId: "agent.test",
          agentName: "Test Agent",
          lastActivity: "2026-08-19T12:00:00Z",
          nativeTranscript: "unavailable",
          projectLabel: "OpenAIDE",
          taskId: "task-1",
          title: "Recent failure",
        }],
        unboundTraces: [],
      };
    }
    if (method === DIAGNOSTICS_CREATE_SUPPORT_EXPORT) {
      return {
        containsSensitiveData: true,
        fileHandleId: "export-1",
        label: "openaide-support.zip",
        sizeBytes: 123,
      };
    }
    throw new Error(`Unexpected method ${method}`);
  });
  const backendConnection = { request } as never;
  installFrontendShell({
    recovery: { openExternal },
    supportExports: { save },
  } as unknown as FrontendShell);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<SupportExportButton connection={backendConnection} taskId="task-1" />));

  await act(async () => document.querySelector<HTMLButtonElement>(".general-support-export")!.click());
  await act(async () => undefined);
  expect(document.body.textContent).toContain("Recent failure");
  expect(document.body.textContent).not.toContain("Native transcript unavailable");
  const continueButton = [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "Continue")!;
  await act(async () => continueButton.click());

  expect(document.querySelector(".support-export-warning")?.textContent).toContain("may contain prompts");

  const exportButton = [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "Export")!;
  await act(async () => exportButton.click());

  expect(request).toHaveBeenCalledWith(DIAGNOSTICS_CREATE_SUPPORT_EXPORT, {
    includeLogs: true,
    includeRuntimeSnapshot: true,
    sessions: [{
      includeAcpTraces: true,
      includeNativeTranscript: false,
      includeOpenaideHistory: true,
      taskId: "task-1",
    }],
    unboundTraceIds: [],
  });
  expect(save).toHaveBeenCalledWith({ fileHandleId: "export-1", label: "openaide-support.zip" });
  expect(openExternal.mock.invocationCallOrder[0]).toBeGreaterThan(save.mock.invocationCallOrder[0]);
});

it("selects sessions in a separate step and preserves them when navigating back", async () => {
  const request = vi.fn(async (method: string) => {
    if (method !== DIAGNOSTICS_LIST_SUPPORT_EXPORT) throw new Error(`Unexpected method ${method}`);
    return {
      acpTraceEnabled: true,
      sessions: [{
        acpTraceCount: 8,
        active: true,
        agentId: "agent.test",
        agentName: "Test Agent",
        lastActivity: "2026-08-19T12:00:00Z",
        nativeTranscript: "unavailable",
        projectLabel: "OpenAIDE",
        taskId: "task-1",
        title: "Recent failure",
      }],
      unboundTraces: [{
        modifiedAt: "2026-08-19T12:01:00Z",
        operation: "session-resume",
        sizeBytes: 3072,
        taskId: "task-orphaned",
        traceId: "trace-1",
      }],
    };
  });
  installFrontendShell({
    recovery: { openExternal: vi.fn() },
    supportExports: { save: vi.fn() },
  } as unknown as FrontendShell);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<SupportExportButton connection={{ request } as never} />));

  await act(async () => document.querySelector<HTMLButtonElement>(".general-support-export")!.click());
  await act(async () => undefined);

  expect(document.body.textContent).toContain("Recent failure");
  expect(document.body.textContent).not.toContain("Runtime snapshot");
  const sessionCheckbox = [...document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')]
    .find((input) => input.parentElement?.textContent?.includes("Recent failure"))!;
  await act(async () => sessionCheckbox.click());

  const continueButton = [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "Continue")!;
  await act(async () => continueButton.click());

  expect(document.body.textContent).toContain("Runtime snapshot");
  expect(document.body.textContent).not.toContain("Recent failure");
  const backButton = [...document.querySelectorAll<HTMLButtonElement>("button")]
    .find((button) => button.textContent === "Back")!;
  await act(async () => backButton.click());

  const restoredSessionCheckbox = [...document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')]
    .find((input) => input.parentElement?.textContent?.includes("Recent failure"))!;
  expect(restoredSessionCheckbox.checked).toBe(true);
});

it("preselects sessions and failed traces updated in the last 15 minutes", async () => {
  const recent = new Date(Date.now() - 5 * 60 * 1_000).toISOString();
  const older = new Date(Date.now() - 20 * 60 * 1_000).toISOString();
  const request = vi.fn(async () => ({
    acpTraceEnabled: true,
    sessions: [
      {
        acpTraceCount: 1,
        active: true,
        agentId: "agent.test",
        agentName: "Test Agent",
        lastActivity: recent,
        nativeTranscript: "unavailable",
        projectLabel: "OpenAIDE",
        taskId: "task-recent",
        title: "Recent session",
      },
      {
        acpTraceCount: 1,
        active: false,
        agentId: "agent.test",
        agentName: "Test Agent",
        lastActivity: older,
        nativeTranscript: "unavailable",
        projectLabel: "OpenAIDE",
        taskId: "task-older",
        title: "Older session",
      },
    ],
    unboundTraces: [
      { modifiedAt: recent, operation: "session-resume", sizeBytes: 512, taskId: "task-failed", traceId: "trace-recent" },
      { modifiedAt: older, operation: "session-load", sizeBytes: 512, taskId: "task-old", traceId: "trace-older" },
    ],
  }));
  installFrontendShell({
    recovery: { openExternal: vi.fn() },
    supportExports: { save: vi.fn() },
  } as unknown as FrontendShell);
  const container = document.createElement("div");
  document.body.append(container);
  const root = createRoot(container);
  await act(async () => root.render(<SupportExportButton connection={{ request } as never} />));

  await act(async () => document.querySelector<HTMLButtonElement>(".general-support-export")!.click());
  await act(async () => undefined);

  const checkbox = (name: string) => [...document.querySelectorAll<HTMLInputElement>('input[type="checkbox"]')]
    .find((input) => input.parentElement?.textContent?.includes(name))!;
  expect(checkbox("Recent session").checked).toBe(true);
  expect(checkbox("Older session").checked).toBe(false);
  expect(checkbox("session-resume").checked).toBe(true);
  expect(checkbox("session-load").checked).toBe(false);
  expect(document.body.textContent).toContain("last 15 minutes");
});
