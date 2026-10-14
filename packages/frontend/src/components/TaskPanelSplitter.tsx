import { useLayoutEffect, useRef, type PointerEvent as ReactPointerEvent } from "react";
import { applyTaskPanelRatio, setLayoutResizing } from "./layoutResize";

/** Divider between Chat and a Task Panel. A drag writes the ratio straight to the Task stack's
 * CSS variable, so the layout follows the pointer without a React render per move; the owner
 * hears the final ratio once. */
export function TaskPanelSplitter({
  label,
  onRatio,
  ratio,
}: {
  label: string;
  onRatio: (ratio: number) => void;
  ratio: number;
}) {
  const drag = useRef<{ latest: number; pointerId: number; start: number; startX: number; stack: HTMLElement; width: number } | undefined>(undefined);
  const current = useRef(ratio);
  current.current = ratio;
  const clamp = (value: number) => Math.min(0.75, Math.max(0.3, value));

  const begin = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.button !== 0) return;
    const stack = event.currentTarget.closest(".task-work-stack");
    if (!(stack instanceof HTMLElement)) return;
    const width = stack.getBoundingClientRect().width;
    if (width <= 0) return;
    event.preventDefault();
    setLayoutResizing(stack, true);
    drag.current = { latest: current.current, pointerId: event.pointerId, start: current.current, startX: event.clientX, stack, width };
    event.currentTarget.setPointerCapture(event.pointerId);
  };
  const move = (event: ReactPointerEvent<HTMLDivElement>) => {
    const state = drag.current;
    if (!state || state.pointerId !== event.pointerId) return;
    state.latest = clamp(state.start - (event.clientX - state.startX) / state.width);
    applyTaskPanelRatio(state.stack, state.latest);
  };
  const finish = (event: ReactPointerEvent<HTMLDivElement>) => {
    const state = drag.current;
    if (!state || state.pointerId !== event.pointerId) return;
    event.currentTarget.releasePointerCapture(event.pointerId);
    setLayoutResizing(state.stack, false);
    drag.current = undefined;
    onRatio(state.latest);
  };
  // A render during a drag must not snap the stack back to the last committed ratio.
  useLayoutEffect(() => {
    const state = drag.current;
    if (!state) return;
    applyTaskPanelRatio(state.stack, state.latest);
    setLayoutResizing(state.stack, true);
  });

  return (
    <div
      aria-label={label}
      aria-orientation="vertical"
      aria-valuemax={75}
      aria-valuemin={30}
      aria-valuenow={Math.round(ratio * 100)}
      className="task-panel-splitter"
      onKeyDown={(event) => {
        if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
        event.preventDefault();
        onRatio(clamp(ratio + (event.key === "ArrowLeft" ? 0.05 : -0.05)));
      }}
      onPointerCancel={finish}
      onPointerDown={begin}
      onPointerMove={move}
      onPointerUp={finish}
      role="separator"
      tabIndex={0}
    />
  );
}
