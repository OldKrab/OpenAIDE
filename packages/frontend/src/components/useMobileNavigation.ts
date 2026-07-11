import { useEffect, useRef, useState, type PointerEvent as ReactPointerEvent } from "react";

type MobileNavigationGesture = {
  captured: boolean;
  pointerId: number;
  progress: number;
  startX: number;
  startY: number;
  startingOpen: boolean;
  tracking: boolean;
  velocityX: number;
  previousX: number;
  previousTime: number;
};

const EDGE_WIDTH = 28;
const CAPTURE_DISTANCE = 8;
const MAX_VERTICAL_DRIFT = 42;
const SETTLE_PROGRESS = 0.42;
const SETTLE_VELOCITY = 0.45;

/** Owns ephemeral, touch-driven presentation state for the mobile task drawer. */
export function useMobileNavigation(enabled: boolean) {
  const [open, setOpen] = useState(false);
  const [dragProgress, setDragProgress] = useState<number>();
  const gestureRef = useRef<MobileNavigationGesture | undefined>(undefined);

  useEffect(() => {
    if (enabled) return;
    gestureRef.current = undefined;
    setDragProgress(undefined);
    setOpen(false);
  }, [enabled]);

  const beginSwipe = (event: ReactPointerEvent<HTMLElement>) => {
    if (!enabled || !isMobileWebViewport()) return;
    if (event.pointerType === "mouse" && event.buttons !== 1) return;
    if (!open && event.clientX > EDGE_WIDTH) return;
    gestureRef.current = {
      captured: false,
      pointerId: event.pointerId,
      progress: open ? 1 : 0,
      startX: event.clientX,
      startY: event.clientY,
      startingOpen: open,
      tracking: true,
      velocityX: 0,
      previousX: event.clientX,
      previousTime: event.timeStamp,
    };
  };

  const trackSwipe = (event: ReactPointerEvent<HTMLElement>) => {
    const gesture = gestureRef.current;
    if (!gesture?.tracking || gesture.pointerId !== event.pointerId) return;
    const deltaX = event.clientX - gesture.startX;
    const deltaY = event.clientY - gesture.startY;
    if (Math.abs(deltaY) > MAX_VERTICAL_DRIFT && Math.abs(deltaY) > Math.abs(deltaX)) {
      gesture.tracking = false;
      return;
    }
    if (!gesture.captured && Math.abs(deltaX) >= CAPTURE_DISTANCE && Math.abs(deltaX) > Math.abs(deltaY) * 1.2) {
      event.currentTarget?.setPointerCapture?.(event.pointerId);
      gesture.captured = true;
    }
    if (!gesture.captured) return;

    const elapsed = event.timeStamp - gesture.previousTime;
    if (elapsed > 0) gesture.velocityX = (event.clientX - gesture.previousX) / elapsed;
    gesture.previousX = event.clientX;
    gesture.previousTime = event.timeStamp;
    const drawerWidth = Math.max(1, Math.min(288, window.innerWidth - 96));
    const rawProgress = (gesture.startingOpen ? 1 : 0) + deltaX / drawerWidth;
    gesture.progress = resistedProgress(rawProgress);
    setDragProgress(gesture.progress);
  };

  const endSwipe = (event: ReactPointerEvent<HTMLElement>) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (gesture.captured) {
      event.currentTarget?.releasePointerCapture?.(event.pointerId);
      const progress = Math.min(1, Math.max(0, gesture.progress));
      const releaseVelocity = event.timeStamp - gesture.previousTime <= 80 ? gesture.velocityX : 0;
      const shouldOpen = releaseVelocity >= SETTLE_VELOCITY
        || (releaseVelocity > -SETTLE_VELOCITY && progress >= SETTLE_PROGRESS);
      setOpen(shouldOpen);
      setDragProgress(undefined);
    }
    gestureRef.current = undefined;
  };

  const cancelSwipe = (event: ReactPointerEvent<HTMLElement>) => {
    const gesture = gestureRef.current;
    if (!gesture || gesture.pointerId !== event.pointerId) return;
    if (gesture.captured) event.currentTarget?.releasePointerCapture?.(event.pointerId);
    setOpen(gesture.startingOpen);
    setDragProgress(undefined);
    gestureRef.current = undefined;
  };

  return {
    active: open || dragProgress !== undefined,
    beginSwipe,
    cancelSwipe,
    dragProgress,
    dragging: dragProgress !== undefined,
    endSwipe,
    open,
    setOpen,
    trackSwipe,
  };
}

function resistedProgress(progress: number) {
  if (progress < 0) return -Math.min(0.025, Math.abs(progress) * 0.08);
  if (progress > 1) return 1 + Math.min(0.025, (progress - 1) * 0.08);
  return progress;
}

function isMobileWebViewport() {
  if (typeof window === "undefined") return false;
  if (typeof window.matchMedia === "function") return window.matchMedia("(max-width: 760px)").matches;
  return window.innerWidth <= 760;
}
