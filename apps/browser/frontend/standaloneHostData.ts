import type { WorkspaceRootSummary } from "@openaide/app-shell-contracts";

const demoProjectRef = "demo-project";
const demoTraceDirectory = "demo-acp-traces";
const demoWorkspaceRoots: WorkspaceRootSummary[] = [{ path: demoProjectRef, label: "Project" }];

export function createStandaloneHostData() {
  return {
    traceDirectory: () => demoTraceDirectory,
    workspaceRoots: () => demoWorkspaceRoots.map((root) => ({ ...root })),
  };
}
