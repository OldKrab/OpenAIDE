import { execFileSync, spawnSync } from "node:child_process";
import { existsSync, readFileSync } from "node:fs";
import path from "node:path";

// Dynamic-DNS, wildcard-DNS, and tunnel services. A hostname under one of these
// names somebody's own machine, so it belongs in an ignored local file.
const PERSONAL_HOST_SUFFIXES = [
  "dedyn.io",
  "duckdns.org",
  "ddns.net",
  "no-ip.com",
  "no-ip.org",
  "no-ip.biz",
  "hopto.org",
  "zapto.org",
  "sytes.net",
  "dynv6.net",
  "dynu.net",
  "nip.io",
  "sslip.io",
  "ts.net",
  "trycloudflare.com",
  "ngrok.io",
  "ngrok.app",
  "ngrok-free.app",
  "ngrok-free.dev",
];

const PERSONAL_HOST_PATTERN = `[a-z0-9-]+\\.(${
  PERSONAL_HOST_SUFFIXES.map((suffix) => suffix.replaceAll(".", "\\.")).join("|")
})([^a-z0-9-]|$)`;

/**
 * Private terms live in the shared Git directory, beside `info/exclude`: every
 * worktree reads the same list and the list itself can never be committed.
 */
export function privateTermsFile(repoRoot) {
  const commonDir = execFileSync("git", ["rev-parse", "--git-common-dir"], {
    cwd: repoRoot,
    encoding: "utf8",
  }).trim();
  return path.resolve(repoRoot, commonDir, "info", "private-terms");
}

export function parsePrivateTerms(source) {
  return source
    .split("\n")
    .map((line) => line.trim())
    .filter((line) => line && !line.startsWith("#"));
}

/**
 * Scans the index, which is exactly what the next commit publishes. Violations
 * carry the location and rule only, so reports never repeat the private value.
 */
export function privateDataViolations(repoRoot) {
  const termsFile = privateTermsFile(repoRoot);
  const terms = existsSync(termsFile) ? parsePrivateTerms(readFileSync(termsFile, "utf8")) : [];
  return [
    ...grepIndex(repoRoot, ["-E", "-e", PERSONAL_HOST_PATTERN], "personal-host"),
    ...(terms.length > 0
      ? grepIndex(repoRoot, ["-F", ...terms.flatMap((term) => ["-e", term])], "private-term")
      : []),
  ].sort((left, right) => left.file.localeCompare(right.file) || left.line - right.line);
}

function grepIndex(repoRoot, patternArgs, rule) {
  const result = spawnSync(
    "git",
    ["grep", "--cached", "--full-name", "-n", "-I", "-i", ...patternArgs],
    { cwd: repoRoot, encoding: "utf8", maxBuffer: 256 * 1024 * 1024 },
  );
  // git grep exits 1 when nothing matches.
  if (result.status === 1) return [];
  if (result.status !== 0) {
    throw new Error(`git grep failed for the ${rule} rule with status ${result.status}`);
  }
  return result.stdout
    .split("\n")
    .map((line) => /^(.*?):(\d+):/.exec(line))
    .filter(Boolean)
    .map(([, file, line]) => ({ file, line: Number(line), rule }));
}
