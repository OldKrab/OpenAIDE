import { Archive, ArrowLeft } from "lucide-react";
import { useRef, useState } from "react";
import type { TaskArchiveOlderCutoff, TaskArchiveOlderResult } from "@openaide/app-server-client";

export type ArchiveOlderTasksAction = (
  cutoff: TaskArchiveOlderCutoff,
  preview: boolean,
) => Promise<TaskArchiveOlderResult>;

type ArchiveOlderState =
  | { kind: "idle" }
  | { kind: "loading" }
  | { kind: "ready"; preview: TaskArchiveOlderResult }
  | { kind: "applying"; preview: TaskArchiveOlderResult }
  | { kind: "complete"; result: TaskArchiveOlderResult }
  | { kind: "error"; message: string; preview?: TaskArchiveOlderResult };

export function useArchiveOlderMenu({
  cutoff,
  onArchiveOlderTasks,
}: {
  cutoff: TaskArchiveOlderCutoff;
  onArchiveOlderTasks?: ArchiveOlderTasksAction;
}) {
  const [state, setState] = useState<ArchiveOlderState>({ kind: "idle" });
  const operationId = useRef(0);

  const begin = async () => {
    if (!onArchiveOlderTasks || state.kind !== "idle") return;
    const currentOperationId = ++operationId.current;
    setState({ kind: "loading" });
    try {
      const preview = await onArchiveOlderTasks(cutoff, true);
      if (currentOperationId === operationId.current) setState({ kind: "ready", preview });
    } catch (error) {
      if (currentOperationId !== operationId.current) return;
      setState({
        kind: "error",
        message: error instanceof Error ? error.message : "Unable to check older tasks.",
      });
    }
  };

  const apply = async () => {
    if (!onArchiveOlderTasks || state.kind !== "ready") return;
    const preview = state.preview;
    const currentOperationId = ++operationId.current;
    setState({ kind: "applying", preview });
    try {
      const result = await onArchiveOlderTasks(cutoff, false);
      if (currentOperationId === operationId.current) setState({ kind: "complete", result });
    } catch (error) {
      if (currentOperationId !== operationId.current) return;
      setState({
        kind: "error",
        message: error instanceof Error ? error.message : "Unable to archive older tasks.",
        preview,
      });
    }
  };

  return {
    apply,
    begin,
    reset: () => {
      operationId.current += 1;
      setState({ kind: "idle" });
    },
    state,
  };
}

export function SidebarArchiveOlderMenu({
  onApply,
  onBack,
  state,
  title,
}: {
  onApply: () => void;
  onBack: () => void;
  state: Exclude<ArchiveOlderState, { kind: "idle" }>;
  title: string;
}) {
  const result = state.kind === "complete" ? state.result
    : state.kind === "ready" || state.kind === "applying" || state.kind === "error" ? state.preview
      : undefined;
  const eligibleCount = result
    ? result.eligibleTaskIds.length + result.eligibleNativeSessions.length
    : undefined;
  const archivedCount = state.kind === "complete"
    ? state.result.archivedTaskIds.length + state.result.archivedNativeSessions.length
    : undefined;
  const protectedCount = result
    ? result.protected.length + result.protectedNativeSessions.length
    : 0;

  return <>
    <button disabled={state.kind === "applying"} onClick={onBack} type="button" role="menuitem">
      <ArrowLeft size={13} />Task actions
    </button>
    <div className="archive-older-confirmation">
      {state.kind === "loading" ? <>
        <strong>Checking older tasks…</strong>
        <p>Using “{title}” as the cutoff.</p>
      </> : state.kind === "complete" ? <>
        <strong>{archivedCount === 1 ? "1 task archived" : `${archivedCount} tasks archived`}</strong>
        <p>They are available in Archive.</p>
        {protectedCount ? <small>{protectedCount} protected {protectedCount === 1 ? "task stayed" : "tasks stayed"} open.</small> : null}
      </> : state.kind === "error" ? <>
        <strong>Couldn’t continue</strong>
        <p className="archive-older-error" role="alert">{state.message}</p>
      </> : eligibleCount === 0 ? <>
        <strong>No older tasks to archive</strong>
        <p>There is nothing before “{title}” that can move.</p>
        {protectedCount ? <small>{protectedCount} protected {protectedCount === 1 ? "task stays" : "tasks stay"} open.</small> : null}
      </> : <>
        <strong>Archive {eligibleCount} older {eligibleCount === 1 ? "task" : "tasks"}?</strong>
        <p>Only tasks before “{title}” in this project will move to Archive.</p>
        {protectedCount ? <small>{protectedCount} protected {protectedCount === 1 ? "task stays" : "tasks stay"} open.</small> : <small>Pinned or busy tasks stay open.</small>}
      </>}
    </div>
    {state.kind === "ready" && eligibleCount ? (
      <button className="archive-older-confirm" onClick={onApply} type="button" role="menuitem">
        <Archive size={13} />Archive {eligibleCount}
      </button>
    ) : state.kind === "applying" ? (
      <button className="archive-older-confirm" disabled type="button" role="menuitem">
        <span className="task-state-spinner" />Archiving…
      </button>
    ) : null}
  </>;
}
