import { existsSync } from "node:fs";
import * as path from "node:path";

export type RuntimeSourceKind = "configured" | "environment" | "bundled" | "development";
export type StorageRootKind = "configured" | "environment" | "extension-storage";

export type RuntimePathInput = {
  extensionRoot: string;
  configuredPath?: string;
  envPath?: string;
  exists?: (candidate: string) => boolean;
  platform?: NodeJS.Platform;
};

export function resolveRuntimePath(input: RuntimePathInput): { kind: RuntimeSourceKind; path: string } {
  if (input.configuredPath) return { kind: "configured", path: input.configuredPath };
  if (input.envPath) return { kind: "environment", path: input.envPath };

  const exists = input.exists ?? existsSync;
  const bundled = path.join(input.extensionRoot, "dist", "app-server", runtimeBinaryName(input.platform));
  if (exists(bundled)) return { kind: "bundled", path: bundled };

  return {
    kind: "development",
    path: path.resolve(input.extensionRoot, "../../target/debug", runtimeBinaryName(input.platform)),
  };
}

export function runtimeBinaryName(platform: NodeJS.Platform = process.platform) {
  return platform === "win32" ? "openaide-app-server.exe" : "openaide-app-server";
}
