import type {
  ActivityToolDetails,
  WorkspaceRootSummary,
} from "@openaide/app-shell-contracts";

const demoProjectRef = "demo-project";
const demoTraceDirectory = "demo-acp-traces";
const demoWorkspaceRoots: WorkspaceRootSummary[] = [{ path: demoProjectRef, label: "Project" }];

export function createDevHostData() {
  return {
    toolDetail: demoToolDetail,
    traceDirectory: () => demoTraceDirectory,
    workspaceRoots: demoWorkspaceRootCopies,
  };
}

function demoWorkspaceRootCopies() {
  return demoWorkspaceRoots.map((root) => ({ ...root }));
}

function demoToolDetail(): ActivityToolDetails {
  return {
    locations: [{ path: "src/components/App.tsx", line: 27 }],
    content: [{ kind: "text", text: "Demo tool detail content from the standalone preview host." }],
  };
}
