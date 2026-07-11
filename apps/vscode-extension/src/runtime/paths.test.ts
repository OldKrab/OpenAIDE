import { describe, expect, it } from "vitest";
import { resolveRuntimePath, runtimeBinaryName } from "./paths";

describe("app server path resolution", () => {
  it("prefers explicit configuration and environment before bundled binaries", () => {
    const configured = resolveRuntimePath({
      extensionRoot: "/extension",
      configuredPath: "/custom/app-server",
      envPath: "/env/app-server",
      exists: () => true,
    });
    expect(configured).toEqual({ kind: "configured", path: "/custom/app-server" });

    const environment = resolveRuntimePath({
      extensionRoot: "/extension",
      envPath: "/env/app-server",
      exists: () => true,
    });
    expect(environment).toEqual({ kind: "environment", path: "/env/app-server" });
  });

  it("uses the bundled extension binary before the development target", () => {
    const resolved = resolveRuntimePath({
      extensionRoot: "/workspace/apps/vscode-extension",
      exists: (candidate) => candidate === "/workspace/apps/vscode-extension/dist/app-server/openaide-app-server",
      platform: "linux",
    });

    expect(resolved).toEqual({
      kind: "bundled",
      path: "/workspace/apps/vscode-extension/dist/app-server/openaide-app-server",
    });
  });

  it("falls back to the workspace development binary", () => {
    const resolved = resolveRuntimePath({
      extensionRoot: "/workspace/apps/vscode-extension",
      exists: () => false,
      platform: "linux",
    });

    expect(resolved.kind).toBe("development");
    expect(resolved.path).toBe("/workspace/target/debug/openaide-app-server");
  });

  it("uses the Windows executable suffix when resolving on win32", () => {
    expect(runtimeBinaryName("win32")).toBe("openaide-app-server.exe");
  });
});
