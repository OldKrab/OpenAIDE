// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { TerminalSummary } from "@openaide/app-server-client";
import type { TerminalIntents } from "../../intents/terminalIntents";
import TerminalPage, { type TerminalPageProps } from "./TerminalPage";
import { TerminalButton } from "./TerminalButton";
import { TerminalSurfaceContext, useTerminalPage } from "./terminalSurface";
import { installFrontendShell, type FrontendShell } from "../../services/frontendShell";

// xterm needs a real layout engine; the page's own behavior is what these tests cover.
vi.mock("./TerminalScreen", () => ({
  TerminalScreen: ({ label, initialAttach }: { label: string; initialAttach?: { token: string } }) => (
    <div data-attach={initialAttach ? "first" : "fresh"} data-testid="screen">{label}</div>
  ),
}));

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;
afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

const summary = (terminalId: string, overrides: Partial<TerminalSummary> = {}): TerminalSummary => ({
  terminalId,
  projectId: "p1" as never,
  folder: "/work/atlas",
  status: "idle",
  openedAt: 1,
  ...overrides,
});

function intents(): TerminalIntents & { [K in keyof TerminalIntents]: ReturnType<typeof vi.fn> } {
  return {
    open: vi.fn(async () => ({ terminal: summary("t-new"), attach: { terminalId: "t-new", token: "first", path: "/terminal" } })),
    attach: vi.fn(),
    close: vi.fn(async () => "closed" as const),
  } as never;
}

async function render(element: React.ReactNode) {
  if (!root) {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  }
  await act(async () => root!.render(element));
}

function page(props: Partial<TerminalPageProps> & Pick<TerminalPageProps, "intents">) {
  return (
    <TerminalPage
      computerName="workstation"
      onBack={() => undefined}
      phone={false}
      projectLabel="Atlas"
      socketUrl={() => "ws://127.0.0.1:1/terminal"}
      target={{ projectId: "p1", taskId: "task-1" }}
      terminals={[]}
      {...props}
    />
  );
}

const button = (name: string) => [...document.querySelectorAll("button")].find((candidate) =>
  candidate.textContent?.trim() === name || candidate.getAttribute("aria-label") === name)!;

it("starts a shell in the Task Workspace when the page opens with none, spending the open's token", async () => {
  const terminalIntents = intents();
  await render(page({ intents: terminalIntents }));
  expect(terminalIntents.open).toHaveBeenCalledWith({ kind: "task", taskId: "task-1" }, { cols: 80, rows: 24 });

  await render(page({ intents: terminalIntents, terminals: [summary("t-new")] }));
  expect(document.querySelector('[data-testid="screen"]')?.getAttribute("data-attach")).toBe("first");
  expect(document.querySelector(".terminal-page-title")?.textContent).toBe("TerminalAtlas · workstation");
  expect(document.querySelector('[role="tab"]')?.textContent).toContain("atlas");
});

it("shows only this Project's Terminals and asks before closing one that runs a command", async () => {
  const terminalIntents = intents();
  const terminals = [summary("t1", { status: "busy", command: "npm" }), summary("t2", { projectId: "p2" as never })];
  await render(page({ intents: terminalIntents, terminals }));

  expect(document.querySelectorAll('[role="tab"]')).toHaveLength(1);
  expect(terminalIntents.open).not.toHaveBeenCalled();
  await act(async () => button("Close npm").click());
  expect(terminalIntents.close).not.toHaveBeenCalled();
  expect(document.querySelector('[role="alertdialog"]')?.textContent).toContain("Stop npm?");

  await act(async () => button("Close terminal").click());
  expect(terminalIntents.close).toHaveBeenCalledWith("t1", true);
});

it("numbers tabs whose titles would repeat", async () => {
  await render(page({ intents: intents(), terminals: [summary("t1"), summary("t2"), summary("t3", { folder: "/work/other" })] }));

  expect([...document.querySelectorAll('[role="tab"] .terminal-tab-title')].map((tab) => tab.textContent))
    .toEqual(["atlas", "atlas 2", "other"]);
});

it("offers a new terminal in the Project when its last one closed", async () => {
  const terminalIntents = intents();
  await render(page({ intents: terminalIntents, terminals: [summary("t1")] }));
  await render(page({ intents: terminalIntents, terminals: [] }));

  expect(button("New terminal in Atlas")).toBeDefined();
  expect(terminalIntents.open).not.toHaveBeenCalled();
});

it("puts the key row on the phone and types its keys into the shell", async () => {
  await render(page({ intents: intents(), phone: true, terminals: [summary("t1")] }));

  expect(document.querySelector('[role="toolbar"][aria-label="Terminal keys"]')).not.toBeNull();
  expect(button("Interrupt").textContent).toBe("^C");
  expect(document.querySelector(".terminal-chips [role=\"tab\"]")?.textContent).toContain("atlas");
});

it("enters from the Task header only where the shell hosts the page", async () => {
  await render(<TerminalButton projectId="p1" taskId="task-1" />);
  expect(document.querySelector("button")).toBeNull();

  const openPage = vi.fn();
  const terminals = [summary("t1"), summary("t2", { status: "exited" }), summary("t3", { projectId: "p2" as never })];
  await render(
    <TerminalSurfaceContext.Provider value={{ terminals, openPage }}>
      <TerminalButton projectId="p1" taskId="task-1" />
    </TerminalSurfaceContext.Provider>,
  );
  expect(button("Terminal, 1 open")).toBeDefined();
  await act(async () => button("Terminal, 1 open").click());
  expect(openPage).toHaveBeenCalledWith({ projectId: "p1", taskId: "task-1" });
});

it("opens the host's own terminal at the Task Workspace where the shell has one", async () => {
  const open = vi.fn();
  installFrontendShell({ nativeTerminal: { open } } as unknown as FrontendShell);
  try {
    await render(<TerminalButton projectId="p1" taskId="task-1" workspaceRoot="/work/atlas" />);
    await act(async () => button("Terminal").click());
    expect(open).toHaveBeenCalledWith("/work/atlas");

    await render(<TerminalButton projectId="p1" taskId="task-1" />);
    expect(document.querySelector("button")).toBeNull();
  } finally {
    installFrontendShell(undefined as never);
  }
});

it("tells the shell how many Terminals are open, for a Quit that warns natively", async () => {
  const openCountChanged = vi.fn();
  installFrontendShell({ terminals: { socketUrl: () => "", openCountChanged } } as unknown as FrontendShell);
  function Workbench({ terminals }: { terminals: TerminalSummary[] }) {
    useTerminalPage("task:1", terminals);
    return null;
  }
  try {
    await render(<Workbench terminals={[summary("t1"), summary("t2", { status: "exited" })]} />);
    await render(<Workbench terminals={[summary("t1"), summary("t3", { projectId: "p2" as never })]} />);
    expect(openCountChanged.mock.calls).toEqual([[1], [2]]);
  } finally {
    installFrontendShell(undefined as never);
  }
});
