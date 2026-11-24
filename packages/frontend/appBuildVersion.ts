import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";
import path from "node:path";

const NEUTRAL_VERSION = "0.0.0";

/**
 * The OpenAIDE version a Frontend build belongs to, baked in by every Vite config that builds
 * the shared Frontend so the connection page can name this app's version.
 *
 * A release build reads the version `scripts/set-release-artifact-version.mjs` stamped into
 * this package's manifest, so it matches the App Server built beside it. Any other build names
 * the version in `package.json` and its commit the way manual packages do (build-vsix.yml):
 * `0.11.0-g22c4bc0`, or `0.12.0-beta.1.g22c4bc0`. Without git the commit reads `dev`.
 */
export function appBuildVersion(repoRoot: string): string {
  const stamped = manifestVersion(path.join(repoRoot, "packages/frontend/package.json"));
  if (stamped !== NEUTRAL_VERSION) return stamped;
  const base = manifestVersion(path.join(repoRoot, "package.json"));
  const sha = commit(repoRoot);
  return `${base}${base.includes("-") ? "." : "-"}${sha ? `g${sha}` : "dev"}`;
}

function manifestVersion(file: string): string {
  return (JSON.parse(readFileSync(file, "utf8")) as { version: string }).version;
}

function commit(repoRoot: string): string | undefined {
  try {
    return execFileSync("git", ["rev-parse", "--short=7", "HEAD"], { cwd: repoRoot, encoding: "utf8", stdio: ["ignore", "pipe", "ignore"] }).trim() || undefined;
  } catch {
    return undefined;
  }
}
