import { useEffect, useRef, useState, type RefObject } from "react";

/**
 * Lets files dropped anywhere on the surrounding Task surface reach its Composer.
 * The surface opts in with `data-composer-drop-scope`; without one the Composer
 * itself is the target. Listeners are native because the scope is an ancestor
 * the Composer does not render.
 */
export function useComposerFileDrop(
  composerRef: RefObject<HTMLElement | null>,
  { disabled, onFiles }: { disabled: boolean; onFiles: (files: File[]) => void },
) {
  const [active, setActive] = useState(false);
  const latest = useRef({ disabled, onFiles });
  latest.current = { disabled, onFiles };

  useEffect(() => {
    const composer = composerRef.current;
    const scope = composer?.closest<HTMLElement>("[data-composer-drop-scope]") ?? composer;
    if (!scope) return;
    const carriesFiles = (event: DragEvent) => Array.from(event.dataTransfer?.types ?? []).includes("Files");
    const over = (event: DragEvent) => {
      if (!carriesFiles(event)) return;
      // Always claim file drags so a missed drop never navigates the app to the file.
      event.preventDefault();
      const accepts = !latest.current.disabled;
      if (event.dataTransfer) event.dataTransfer.dropEffect = accepts ? "copy" : "none";
      setActive(accepts);
    };
    const leave = (event: DragEvent) => {
      const next = event.relatedTarget as Node | null;
      if (!next || !scope.contains(next)) setActive(false);
    };
    const drop = (event: DragEvent) => {
      setActive(false);
      if (!carriesFiles(event)) return;
      event.preventDefault();
      if (latest.current.disabled) return;
      const files = Array.from(event.dataTransfer?.files ?? []);
      if (files.length > 0) latest.current.onFiles(files);
    };
    scope.addEventListener("dragover", over);
    scope.addEventListener("dragleave", leave);
    scope.addEventListener("drop", drop);
    return () => {
      scope.removeEventListener("dragover", over);
      scope.removeEventListener("dragleave", leave);
      scope.removeEventListener("drop", drop);
    };
  }, [composerRef]);

  return active;
}
