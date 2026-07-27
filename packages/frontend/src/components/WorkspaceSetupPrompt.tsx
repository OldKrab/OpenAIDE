import { FolderOpen } from "lucide-react";

/** Presents the shell-provided recovery action when a Task cannot start without a workspace. */
export function WorkspaceSetupPrompt({
  compact = false,
  onOpenFolder,
}: {
  compact?: boolean;
  onOpenFolder: () => void;
}) {
  return (
    <div className={`workspace-setup-prompt ${compact ? "compact" : ""}`}>
      <FolderOpen aria-hidden="true" size={compact ? 16 : 20} />
      <div className="workspace-setup-copy">
        <strong>Open a folder to start a task</strong>
        <span>OpenAIDE needs a workspace folder before it can prepare a task.</span>
      </div>
      <button className="workspace-setup-action" onClick={onOpenFolder} type="button">
        <FolderOpen aria-hidden="true" size={14} />
        Open Folder
      </button>
    </div>
  );
}
