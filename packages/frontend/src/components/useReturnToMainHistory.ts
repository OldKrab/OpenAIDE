import { useEffect } from "react";

/**
 * Alt+Left returns from a subagent history to the Task's main history. The shortcut only
 * exists while a subagent is selected, so it cannot steal the key elsewhere.
 */
export function useReturnToMainHistory(
  selectedSubagentId: string | undefined,
  selectSubagent: (subagentId?: string) => void,
) {
  useEffect(() => {
    if (!selectedSubagentId) return;
    const returnToMain = (event: KeyboardEvent) => {
      if (!event.altKey || event.key !== "ArrowLeft" || event.ctrlKey || event.metaKey || event.shiftKey) {
        return;
      }
      event.preventDefault();
      selectSubagent(undefined);
    };
    window.addEventListener("keydown", returnToMain);
    return () => window.removeEventListener("keydown", returnToMain);
  }, [selectSubagent, selectedSubagentId]);
}
