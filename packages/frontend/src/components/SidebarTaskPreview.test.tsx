import { act, create } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import { SidebarTaskPreviewProvider, taskPreviewContent, useSidebarTaskPreview } from "./SidebarTaskPreview";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("SidebarTaskPreview", () => {
  it("keeps task previews available in a narrow VS Code sidebar", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      innerHeight: 800,
      innerWidth: 280,
      matchMedia: () => ({ matches: true }),
    });
    vi.stubGlobal("document", {
      body: { dataset: { shell: "vscodeExtension" } },
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    const row = {
      getBoundingClientRect: () => ({ bottom: 72, height: 32, left: 4, right: 276, top: 40, width: 272, x: 4, y: 40 }),
    } as HTMLElement;
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<SidebarTaskPreviewProvider><HoverTarget row={row} /></SidebarTaskPreviewProvider>);
    });

    act(() => tree.root.findByType("button").props.onPointerEnter());
    act(() => {
      vi.advanceTimersByTime(1_000);
    });

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);
  });

  it("opens after one second of pointer dwell", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      innerHeight: 800,
      innerWidth: 1200,
      matchMedia: () => ({ matches: false }),
    });
    vi.stubGlobal("document", {
      addEventListener: vi.fn(),
      removeEventListener: vi.fn(),
    });
    const row = {
      getBoundingClientRect: () => ({ bottom: 72, height: 32, left: 8, right: 296, top: 40, width: 288, x: 8, y: 40 }),
    } as HTMLElement;
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<SidebarTaskPreviewProvider><HoverTarget row={row} /></SidebarTaskPreviewProvider>);
    });

    act(() => tree.root.findByType("button").props.onPointerEnter());
    act(() => {
      vi.advanceTimersByTime(999);
    });
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);
  });
});

function HoverTarget({ row }: { row: HTMLElement }) {
  const preview = useSidebarTaskPreview();
  return <button onPointerEnter={() => preview?.enter(taskPreviewContent(task()), row)} type="button">Task</button>;
}

function task(): TaskSummary {
  return {
    task_id: "task_1",
    project_id: "project_1",
    project_label: "OpenAIDE",
    agent_id: "codex",
    agent_name: "Codex",
    title: "Task",
    status: "inactive",
    task_version: 1,
    message_history_version: 1,
    has_messages: true,
    created_at: "1",
    updated_at: "1",
    last_activity: "1",
    unread: false,
    workspace_root: "/workspace/OpenAIDE",
    isolation: "local",
    workspace_available: true,
  };
}
