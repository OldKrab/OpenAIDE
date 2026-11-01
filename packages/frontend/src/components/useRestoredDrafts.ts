import { useEffect, useRef, type Dispatch, type RefObject } from "react";
import { currentComputer, useComputers, type ComputersCapability } from "../services/computers";
import { draftCount, takeKeptDrafts } from "../services/keptDrafts";
import type { AppAction } from "../state/appReducer";
import type { AppState } from "../state/store";

/**
 * Puts draft text kept by a computer switch back once this page knows its computer
 * and its App Server is ready, since a new state root resets Task inputs (ADR-0070).
 * Text typed in the meantime wins over the kept copy.
 */
export function useRestoredDrafts({ backendReady, capability, dispatch, latestState }: {
  backendReady: boolean;
  capability: ComputersCapability | undefined;
  dispatch: Dispatch<AppAction>;
  latestState: RefObject<Pick<AppState, "newTask" | "taskInputs">>;
}) {
  const snapshot = useComputers(capability);
  const restored = useRef(false);
  useEffect(() => {
    if (!backendReady || !snapshot || restored.current) return;
    restored.current = true;
    const drafts = takeKeptDrafts(currentComputer(snapshot)?.id ?? "");
    if (!drafts) return;
    const { newTask, taskInputs } = latestState.current;
    if (drafts.newTask !== undefined && !newTask.prompt) dispatch({ type: "prompt", prompt: drafts.newTask });
    for (const [taskId, prompt] of Object.entries(drafts.tasks)) {
      if (!taskInputs[taskId]?.prompt) dispatch({ type: "taskInput:prompt", taskId, prompt });
    }
    console.info(`computer_switch_drafts_restored count=${draftCount(drafts)}`);
  }, [backendReady, dispatch, latestState, snapshot]);
}
