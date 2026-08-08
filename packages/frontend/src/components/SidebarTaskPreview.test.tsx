import { act, create } from "react-test-renderer";
import { afterEach, describe, expect, it, vi } from "vitest";
import type { TaskSummary } from "@openaide/app-shell-contracts";
import {
  createSidebarPreviewCoordinator,
  SidebarTaskPreviewProvider,
  taskPreviewContent,
  useSidebarTaskPreview,
} from "./SidebarTaskPreview";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

afterEach(() => {
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("SidebarTaskPreview", () => {
  it("transfers an open preview immediately between different row kinds", () => {
    const coordinator = createSidebarPreviewCoordinator();
    const taskOwner = Symbol("task");
    const projectOwner = Symbol("project");
    const dismissTask = vi.fn(() => coordinator.closed(taskOwner));

    expect(coordinator.enter(taskOwner)).toBe(false);
    coordinator.opened(taskOwner, dismissTask);

    expect(coordinator.enter(projectOwner)).toBe(true);
    expect(dismissTask).toHaveBeenCalledOnce();
  });

  it("does not open rich hover previews inside VS Code navigation", () => {
    vi.useFakeTimers();
    vi.stubGlobal("window", {
      innerHeight: 800,
      innerWidth: 900,
      matchMedia: () => ({ matches: false }),
    });
    vi.stubGlobal("document", {
      body: { dataset: { shell: "vscodeExtension" } },
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
      vi.advanceTimersByTime(1_000);
    });

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
  });

  it("opens after 750 milliseconds of pointer dwell", () => {
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
      vi.advanceTimersByTime(749);
    });
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);

    act(() => {
      vi.advanceTimersByTime(1);
    });
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);
  });

  it("closes immediately when the pointer leaves for blank space", () => {
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
      vi.advanceTimersByTime(750);
    });
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);

    act(() => tree.root.findByType("button").props.onPointerLeave({ relatedTarget: null }));

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
  });

  it("stays open when the pointer moves directly into the preview", () => {
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
    const previewNode = {} as HTMLDivElement;
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(
        <SidebarTaskPreviewProvider><HoverTarget row={row} /></SidebarTaskPreviewProvider>,
        {
          createNodeMock: (element) => (
            (element.props as { role?: string }).role === "dialog" ? previewNode : null
          ),
        },
      );
    });

    act(() => tree.root.findByType("button").props.onPointerEnter());
    act(() => {
      vi.advanceTimersByTime(750);
    });
    act(() => tree.root.findByType("button").props.onPointerLeave({ relatedTarget: previewNode }));

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);
  });

  it("closes immediately when its owning task list scrolls", () => {
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
    let onScroll: EventListener | undefined;
    const taskList = {
      addEventListener: vi.fn((type: string, listener: EventListener) => {
        if (type === "scroll") onScroll = listener;
      }),
      removeEventListener: vi.fn(),
    };
    const row = {
      closest: () => taskList,
      getBoundingClientRect: () => ({ bottom: 72, height: 32, left: 8, right: 296, top: 40, width: 288, x: 8, y: 40 }),
    } as unknown as HTMLElement;
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(<SidebarTaskPreviewProvider><HoverTarget row={row} /></SidebarTaskPreviewProvider>);
    });

    act(() => tree.root.findByType("button").props.onPointerEnter());
    act(() => {
      vi.advanceTimersByTime(750);
    });
    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(1);

    act(() => onScroll?.(new Event("scroll")));

    expect(tree.root.findAllByProps({ role: "dialog" })).toHaveLength(0);
  });
});

function HoverTarget({ row }: { row: HTMLElement }) {
  const preview = useSidebarTaskPreview();
  return <button
    onPointerEnter={() => preview?.enter(taskPreviewContent(task()), row)}
    onPointerLeave={(event) => preview?.leave(event.relatedTarget)}
    type="button"
  >Task</button>;
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
    pinned: false,
    workspace_root: "/workspace/OpenAIDE",
    isolation: "local",
    workspace_available: true,
  };
}
