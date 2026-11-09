import { useEffect, useRef, type MouseEvent as ReactMouseEvent, type PointerEvent as ReactPointerEvent } from "react";

/** Hold time before a touch counts as a long press; close to Android's system long-press timeout. */
export const LONG_PRESS_DELAY_MS = 450;
/** Finger travel that turns a press into a scroll or swipe and cancels it. */
export const LONG_PRESS_SLOP_PX = 10;

/**
 * Calls `onLongPress` when a touch is held still on the element. Mouse and pen input are
 * ignored, so pointer shells keep their own controls. The tap that ends a long press is
 * swallowed so it does not also activate the row, and the platform's own long-press
 * menu or text selection is suppressed while a touch is down.
 */
export function useTouchLongPress(onLongPress: () => void, enabled = true) {
  const pressRef = useRef<{ pointerId: number; x: number; y: number; timer: ReturnType<typeof setTimeout> } | undefined>(undefined);
  const firedRef = useRef(false);
  const touchDownRef = useRef(false);
  const callbackRef = useRef(onLongPress);
  callbackRef.current = onLongPress;

  const cancel = () => {
    if (pressRef.current) clearTimeout(pressRef.current.timer);
    pressRef.current = undefined;
  };
  useEffect(() => cancel, []);

  return {
    onPointerDown(event: ReactPointerEvent<HTMLElement>) {
      cancel();
      touchDownRef.current = event.pointerType === "touch";
      firedRef.current = false;
      if (!enabled || event.pointerType !== "touch" || !event.isPrimary) return;
      pressRef.current = {
        pointerId: event.pointerId,
        x: event.clientX,
        y: event.clientY,
        timer: setTimeout(() => {
          pressRef.current = undefined;
          firedRef.current = true;
          callbackRef.current();
        }, LONG_PRESS_DELAY_MS),
      };
    },
    onPointerMove(event: ReactPointerEvent<HTMLElement>) {
      const press = pressRef.current;
      if (!press || press.pointerId !== event.pointerId) return;
      if (Math.hypot(event.clientX - press.x, event.clientY - press.y) > LONG_PRESS_SLOP_PX) cancel();
    },
    onPointerUp: cancel,
    onPointerCancel: cancel,
    onClickCapture(event: ReactMouseEvent<HTMLElement>) {
      if (!firedRef.current) return;
      firedRef.current = false;
      event.preventDefault();
      event.stopPropagation();
    },
    onContextMenu(event: ReactMouseEvent<HTMLElement>) {
      if (enabled && touchDownRef.current) event.preventDefault();
    },
  };
}
