import { useEffect } from "react";
import { useBackNavigation } from "./useBackNavigation";

/**
 * Alt+Left and Android Back return from a subagent history to the Task's main history. Both
 * only exist while a subagent is selected, so they cannot steal the key elsewhere. Back ranks
 * below every overlay, so whatever covers the history closes first.
 */
export function useReturnToMainHistory(
  selectedSubagentId: string | undefined,
  selectSubagent: (subagentId?: string) => void,
) {
  useBackNavigation(Boolean(selectedSubagentId), () => selectSubagent(undefined), 10);
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
