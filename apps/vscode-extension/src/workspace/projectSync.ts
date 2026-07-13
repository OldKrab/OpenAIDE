import * as vscode from "vscode";
import { workspaceRoots } from "./roots";

type WorkspaceProjectRuntime = {
  syncWorkspaceRoots(roots: Array<{ path: string }>): Promise<void>;
};

type WorkspaceProjectLogger = {
  warn(message: string, fields?: Record<string, unknown>): void;
};

export type WorkspaceProjectSync = vscode.Disposable & {
  /** Settles after the first root set is reported, including recoverable failures. */
  ready: Promise<void>;
};

/** Keeps the App Server's Project registry aligned with VS Code's live folders. */
export function registerWorkspaceProjectSync(
  runtime: WorkspaceProjectRuntime,
  logger: WorkspaceProjectLogger,
): WorkspaceProjectSync {
  const sync = async () => {
    try {
      await runtime.syncWorkspaceRoots(workspaceRoots().map(({ path }) => ({ path })));
    } catch (error) {
      // A later folder change retries the full replacement set; webviews remain usable meanwhile.
      logger.warn("failed to synchronize VS Code workspace Projects", { error: String(error) });
    }
  };
  const listener = vscode.workspace.onDidChangeWorkspaceFolders(() => {
    void sync();
  });

  return {
    ready: sync(),
    dispose: () => listener.dispose(),
  };
}
