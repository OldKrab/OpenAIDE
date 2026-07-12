/** Derives the App Server-compatible identity for a canonical local workspace path. */
export function projectIdForWorkspaceRoot(workspaceRoot: string) {
  return projectIdFromCanonicalRoot(canonicalWorkspaceRoot(workspaceRoot));
}

function projectIdFromCanonicalRoot(workspaceRoot: string) {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  for (const byte of new TextEncoder().encode(workspaceRoot)) {
    hash ^= BigInt(byte);
    hash = (hash * prime) & mask;
  }
  return `project-${hash.toString(16).padStart(16, "0")}`;
}

function canonicalWorkspaceRoot(workspaceRoot: string) {
  if (!workspaceRoot) return "";
  const absolute = workspaceRoot.startsWith("/") || /^[A-Za-z]:[\\/]/u.test(workspaceRoot);
  const prefix = workspaceRoot.startsWith("/") ? "/" : "";
  const normalized: string[] = [];
  for (const part of workspaceRoot.split(/[\\/]+/u)) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (normalized.length > 0) normalized.pop();
      else if (!absolute) normalized.push("..");
      continue;
    }
    normalized.push(part);
  }
  return `${prefix}${normalized.join("/")}` || ".";
}
