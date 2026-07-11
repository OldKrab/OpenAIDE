export function projectIdForWorkspaceRoot(workspaceRoot: string) {
  return projectIdFromCanonicalRoot(canonicalWorkspaceRoot(workspaceRoot));
}

export function workspaceLabel(workspaceRoot: string) {
  const trimmedSeparators = workspaceRoot.replace(/[\\/]+$/u, "");
  const segments = trimmedSeparators.split(/[\\/]+/u);
  return segments.at(-1)?.trim() || "Project";
}

export function workspaceRootForProjectId(projectId: string | undefined, workspaceRoot: string) {
  const trimmedRoot = workspaceRoot.trim();
  if (!projectId || !trimmedRoot) return undefined;
  return projectIdForWorkspaceRoot(trimmedRoot) === projectId ? trimmedRoot : undefined;
}

function projectIdFromCanonicalRoot(workspaceRoot: string) {
  let hash = 0xcbf29ce484222325n;
  const prime = 0x100000001b3n;
  const mask = 0xffffffffffffffffn;
  const bytes = new TextEncoder().encode(workspaceRoot);
  for (const byte of bytes) {
    hash ^= BigInt(byte);
    hash = (hash * prime) & mask;
  }
  return `project-${hash.toString(16).padStart(16, "0")}`;
}

function canonicalWorkspaceRoot(workspaceRoot: string) {
  if (!workspaceRoot) return "";
  const absolute = workspaceRoot.startsWith("/") || /^[A-Za-z]:[\\/]/u.test(workspaceRoot);
  const prefix = workspaceRoot.startsWith("/") ? "/" : "";
  const parts = workspaceRoot.split(/[\\/]+/u);
  const normalized: string[] = [];
  for (const part of parts) {
    if (!part || part === ".") continue;
    if (part === "..") {
      if (normalized.length > 0) {
        normalized.pop();
      } else if (!absolute) {
        normalized.push("..");
      }
      continue;
    }
    normalized.push(part);
  }
  const path = `${prefix}${normalized.join("/")}`;
  return path || ".";
}
