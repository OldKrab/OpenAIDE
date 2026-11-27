import { act, create } from "react-test-renderer";
import { beforeEach, describe, expect, it, vi } from "vitest";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import { SidebarNativeSessionRow } from "./SidebarNativeSessionRow";
import { SidebarTaskPreviewProvider } from "./SidebarTaskPreview";

const mounts = vi.hoisted(() => ({ agentIcon: 0 }));

vi.mock("./AgentIcon", async () => {
  const react = await import("react");
  return {
    AgentIcon: () => {
      react.useEffect(() => {
        mounts.agentIcon += 1;
      }, []);
      return null;
    },
  };
});

beforeEach(() => {
  (globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;
  mounts.agentIcon = 0;
});

function row(session: AgentListedSession) {
  return (
    <SidebarTaskPreviewProvider>
      <SidebarNativeSessionRow
        archived={false}
        nativeSessionAgentId="claude-code"
        nativeSessionAgentName="Claude Code"
        onArchiveNativeSession={vi.fn()}
        onOpenNativeSession={vi.fn()}
        onRestoreNativeSession={vi.fn()}
        onSetNativeSessionPinned={vi.fn()}
        onSetNativeSessionTitle={vi.fn()}
        session={session}
      />
    </SidebarTaskPreviewProvider>
  );
}

describe("SidebarNativeSessionRow", () => {
  it("keeps the row's content mounted across renders", () => {
    // Replacing the content under a resting pointer makes the browser drop the click and
    // re-enter the hover preview, which re-renders the row again without end.
    const session = { session_id: "session_1", title: "Relay setup" } as AgentListedSession;
    let tree!: ReturnType<typeof create>;
    act(() => {
      tree = create(row(session));
    });
    expect(mounts.agentIcon).toBe(1);

    act(() => tree.update(row({ ...session, last_activity: "2026-11-27T10:00:00.000Z" })));
    act(() => tree.update(row({ ...session, title: "Relay setup and logs" })));

    expect(mounts.agentIcon).toBe(1);
  });
});
