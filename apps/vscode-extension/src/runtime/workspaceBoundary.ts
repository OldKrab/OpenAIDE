import * as path from "node:path";
import { lstat, realpath } from "node:fs/promises";
import * as vscode from "vscode";

export async function validatedWorkspacePath(filePath: string, mode: "existing" | "write-target") {
  if (!path.isAbsolute(filePath)) {
    throw new Error("path must be absolute");
  }

  const resolved = path.resolve(filePath);
  const realResolved =
    mode === "existing" ? await realExistingPath(resolved) : await realExistingPathOrNearestParent(resolved);
  const roots = vscode.workspace.workspaceFolders ?? [];
  const realRoots = await Promise.all(roots.map((folder) => realExistingPath(folder.uri.fsPath)));
  const allowed = realRoots.some((root) => isInsideRoot(realResolved, root));
  if (!allowed) {
    throw new Error("path is outside the current workspace");
  }

  return resolved;
}

export function isFileNotFound(error: unknown) {
  if (!error || typeof error !== "object") return false;
  const code = "code" in error ? (error as { code?: unknown }).code : undefined;
  const name = "name" in error ? (error as { name?: unknown }).name : undefined;
  return code === "ENOENT" || code === "FileNotFound" || name === "FileNotFound";
}

async function realExistingPath(filePath: string) {
  try {
    return await realpath(filePath);
  } catch (error) {
    if (isFileNotFound(error)) {
      throw new Error("path does not exist");
    }
    throw error;
  }
}

async function realExistingPathOrNearestParent(filePath: string) {
  let candidate = filePath;
  while (true) {
    try {
      const realCandidate = await realpath(candidate);
      const relativeTail = path.relative(candidate, filePath);
      return path.resolve(realCandidate, relativeTail);
    } catch (error) {
      if (!isFileNotFound(error)) {
        throw error;
      }
      // realpath reports ENOENT for dangling symlinks as well as absent paths.
      // Falling back to their lexical parent would authorize creation through
      // an unresolved link whose target can be outside every workspace root.
      const entry = await lstat(candidate).catch((statError: unknown) => {
        if (isFileNotFound(statError)) return undefined;
        throw statError;
      });
      if (entry?.isSymbolicLink()) {
        throw new Error("Cannot write through an unresolved symbolic link.");
      }
      const parent = path.dirname(candidate);
      if (parent === candidate) {
        throw new Error("path does not exist");
      }
      candidate = parent;
    }
  }
}

function isInsideRoot(filePath: string, rootPath: string) {
  const relative = path.relative(path.resolve(rootPath), filePath);
  return relative === "" || (
    relative !== ".." && !relative.startsWith(`..${path.sep}`) && !path.isAbsolute(relative)
  );
}
