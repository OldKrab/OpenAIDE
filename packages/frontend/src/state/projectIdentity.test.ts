import { describe, expect, it } from "vitest";
import { projectIdForWorkspaceRoot, workspaceLabel } from "./projectIdentity";

describe("projectIdentity", () => {
  it("matches the App Server project identity for normalized workspace roots", () => {
    expect(projectIdForWorkspaceRoot("/workspace/app")).toBe("project-b01b10a03e5d25eb");
    expect(projectIdForWorkspaceRoot("/workspace/app/./src/..")).toBe("project-b01b10a03e5d25eb");
    expect(workspaceLabel("/workspace/app/")).toBe("app");
  });
});
