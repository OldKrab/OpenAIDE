import type { WorkspaceRootSummary } from "@openaide/app-shell-contracts";

const demoProjectRef = "demo-project";
const demoWorkspaceRoots: WorkspaceRootSummary[] = [{ path: demoProjectRef, label: "Project" }];

export function createStandaloneHostData() {
  return {
    workspaceRoots: () => demoWorkspaceRoots.map((root) => ({ ...root })),
  };
}
