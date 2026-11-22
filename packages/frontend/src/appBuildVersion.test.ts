import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { describe, expect, it } from "vitest";
import { appBuildVersion } from "../appBuildVersion";

describe("appBuildVersion", () => {
  it("names a release by its stamped version", () => {
    expect(appBuildVersion(fixture("0.12.0", "0.12.0"))).toBe("0.12.0");
  });

  it("names any other build by the last release and its commit, or dev without git", () => {
    expect(appBuildVersion(fixture("0.0.0", "0.11.0"))).toBe("0.11.0-dev");
    expect(appBuildVersion(fixture("0.0.0", "0.12.0-beta.1"))).toBe("0.12.0-beta.1.dev");
    expect(appBuildVersion(path.resolve(import.meta.dirname, "../../.."))).toMatch(/^\d+\.\d+\.\d+(-[a-z]+\.\d+)?[-.](g[0-9a-f]{7,}|dev)$/);
  });
});

function fixture(frontendVersion: string, rootVersion: string) {
  const root = mkdtempSync(path.join(os.tmpdir(), "openaide-app-version-"));
  mkdirSync(path.join(root, "packages/frontend"), { recursive: true });
  writeFileSync(path.join(root, "package.json"), JSON.stringify({ version: rootVersion }));
  writeFileSync(path.join(root, "packages/frontend/package.json"), JSON.stringify({ version: frontendVersion }));
  return root;
}
