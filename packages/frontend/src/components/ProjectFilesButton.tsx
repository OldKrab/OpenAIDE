import { Folder } from "lucide-react";
import { createPortal } from "react-dom";

export function ProjectFilesButton({ open, onOpen, target }: {
  open: boolean;
  onOpen(): void;
  target?: HTMLElement | null;
}) {
  const button = (
    <button type="button" className="project-files-entry" aria-label="Project files" title="Project files" aria-expanded={open} onClick={onOpen}>
      {target ? <Folder aria-hidden="true" size={18} /> : "Project files"}
    </button>
  );
  return target ? createPortal(button, target) : button;
}
