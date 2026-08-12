import { Minus, Plus, X } from "lucide-react";
import {
  useEffect,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
  type MouseEvent,
  type PointerEvent as ReactPointerEvent,
  type ReactNode,
  type WheelEvent,
} from "react";

export type ImagePreviewViewportSource = {
  label: string;
  url: string;
};

const MIN_SCALE = 0.25;
const FIT_SCALE = 1;
const MAX_SCALE = 5;
const ZOOM_STEP = 0.25;
const WHEEL_ZOOM_FACTOR_PER_100_DELTA = 1.25;

type ImageView = {
  scale: number;
  x: number;
  y: number;
};

const FITTED_VIEW: ImageView = { scale: FIT_SCALE, x: 0, y: 0 };

/** Provides bounded zoom and pan controls for the shared image inspection surface. */
export function ImagePreviewViewport({
  image,
  onClose,
  contentNoun = "image",
  imageClassName = "attachment-preview-image",
  toolbarActions,
}: {
  image: ImagePreviewViewportSource;
  onClose?: () => void;
  contentNoun?: string;
  imageClassName?: string;
  toolbarActions?: ReactNode;
}) {
  const stageRef = useRef<HTMLDivElement>(null);
  const imageRef = useRef<HTMLImageElement>(null);
  const pointersRef = useRef(new Map<number, { x: number; y: number }>());
  const movedRef = useRef(false);
  const pressedImageRef = useRef(false);
  const [view, setView] = useState(FITTED_VIEW);
  const [interacting, setInteracting] = useState(false);

  useEffect(() => {
    const stage = stageRef.current;
    if (!stage) return undefined;
    const preventPageScroll = (event: globalThis.WheelEvent) => event.preventDefault();
    stage.addEventListener("wheel", preventPageScroll, { passive: false });
    return () => stage.removeEventListener("wheel", preventPageScroll);
  }, []);

  const zoomAt = (
    nextScale: number | ((currentScale: number) => number),
    clientX?: number,
    clientY?: number,
  ) => {
    setView((current) => {
      const requestedScale = typeof nextScale === "function" ? nextScale(current.scale) : nextScale;
      const scale = clamp(requestedScale, MIN_SCALE, MAX_SCALE);
      const stage = stageRef.current;
      if (!stage) return { ...current, scale };
      const origin = imageLayoutOrigin(stage, imageRef.current, current);
      const focusX = (clientX ?? origin.x + current.x) - origin.x;
      const focusY = (clientY ?? origin.y + current.y) - origin.y;
      const ratio = scale / current.scale;
      return constrainView({
        scale,
        x: focusX - (focusX - current.x) * ratio,
        y: focusY - (focusY - current.y) * ratio,
      }, stage, imageRef.current);
    });
  };

  const onWheel = (event: WheelEvent<HTMLDivElement>) => {
    // Trackpads split one gesture into many small wheel events, so zoom must depend on
    // accumulated movement rather than event count.
    const factor = WHEEL_ZOOM_FACTOR_PER_100_DELTA ** (-event.deltaY / 100);
    zoomAt((currentScale) => currentScale * factor, event.clientX, event.clientY);
  };

  const onPointerDown = (event: ReactPointerEvent<HTMLDivElement>) => {
    if (event.pointerType === "mouse" && event.button !== 0) return;
    event.preventDefault();
    event.currentTarget.setPointerCapture(event.pointerId);
    if (pointersRef.current.size === 0) {
      movedRef.current = false;
      pressedImageRef.current = event.target === imageRef.current;
    }
    pointersRef.current.set(event.pointerId, { x: event.clientX, y: event.clientY });
    setInteracting(true);
  };

  const onPointerMove = (event: ReactPointerEvent<HTMLDivElement>) => {
    const previous = pointersRef.current.get(event.pointerId);
    if (!previous) return;
    const previousPair = pointerPair(pointersRef.current);
    const next = { x: event.clientX, y: event.clientY };
    if (Math.hypot(next.x - previous.x, next.y - previous.y) > 3) movedRef.current = true;
    pointersRef.current.set(event.pointerId, next);
    const nextPair = pointerPair(pointersRef.current);
    if (previousPair && nextPair) {
      setView((current) => {
        const stage = stageRef.current;
        if (!stage) return current;
        const previousDistance = pointerDistance(previousPair);
        if (previousDistance === 0) return current;
        const scale = clamp(
          current.scale * pointerDistance(nextPair) / previousDistance,
          MIN_SCALE,
          MAX_SCALE,
        );
        const origin = imageLayoutOrigin(stage, imageRef.current, current);
        const previousMidpoint = pointerMidpoint(previousPair, origin);
        const nextMidpoint = pointerMidpoint(nextPair, origin);
        const ratio = scale / current.scale;
        return constrainView({
          scale,
          x: nextMidpoint.x - (previousMidpoint.x - current.x) * ratio,
          y: nextMidpoint.y - (previousMidpoint.y - current.y) * ratio,
        }, stage, imageRef.current);
      });
      return;
    }
    if (pointersRef.current.size !== 1) return;
    setView((current) => {
      const stage = stageRef.current;
      if (!stage) return current;
      return constrainView({
        ...current,
        x: current.x + next.x - previous.x,
        y: current.y + next.y - previous.y,
      }, stage, imageRef.current);
    });
  };

  const onPointerEnd = (event: ReactPointerEvent<HTMLDivElement>) => {
    pointersRef.current.delete(event.pointerId);
    event.currentTarget.releasePointerCapture(event.pointerId);
    if (pointersRef.current.size === 0) setInteracting(false);
  };

  const onDoubleClick = (event: MouseEvent<HTMLDivElement>) => {
    event.preventDefault();
    zoomAt(view.scale === FIT_SCALE ? 2 : FIT_SCALE, event.clientX, event.clientY);
  };

  const onClick = (event: MouseEvent<HTMLDivElement>) => {
    const handledImageGesture = movedRef.current || pressedImageRef.current;
    movedRef.current = false;
    pressedImageRef.current = false;
    if (handledImageGesture) return;
    if (event.target === event.currentTarget) onClose?.();
  };

  const onKeyDown = (event: KeyboardEvent<HTMLDivElement>) => {
    if (event.key === "+" || event.key === "=") {
      event.preventDefault();
      zoomAt(view.scale + ZOOM_STEP);
      return;
    }
    if (event.key === "-") {
      event.preventDefault();
      zoomAt(view.scale - ZOOM_STEP);
      return;
    }
    if (event.key === "0") {
      event.preventDefault();
      setView(FITTED_VIEW);
      return;
    }
    const movement = {
      ArrowDown: { x: 0, y: -40 },
      ArrowLeft: { x: 40, y: 0 },
      ArrowRight: { x: -40, y: 0 },
      ArrowUp: { x: 0, y: 40 },
    }[event.key];
    if (!movement) return;
    event.preventDefault();
    setView((current) => {
      const stage = stageRef.current;
      if (!stage) return current;
      return constrainView({
        ...current,
        x: current.x + movement.x,
        y: current.y + movement.y,
      }, stage, imageRef.current);
    });
  };

  const imageStyle: CSSProperties = {
    transform: `translate3d(${view.x}px, ${view.y}px, 0) scale(${view.scale})`,
  };
  const isCenteredFit = view.scale === FIT_SCALE && view.x === 0 && view.y === 0;

  return (
    <>
      <div className="attachment-preview-chrome">
        <span className="attachment-preview-label" title={image.label}>{image.label}</span>
        <div className="attachment-preview-actions">
          <button
            aria-label={`Zoom ${contentNoun} out`}
            className="attachment-preview-action"
            disabled={view.scale <= MIN_SCALE}
            onClick={() => zoomAt(view.scale - ZOOM_STEP)}
            type="button"
          >
            <Minus aria-hidden="true" size={16} />
          </button>
          <button
            aria-label={`Reset ${contentNoun} zoom`}
            className="attachment-preview-zoom"
            disabled={isCenteredFit}
            onClick={() => setView(FITTED_VIEW)}
            type="button"
          >
            {view.scale === FIT_SCALE ? "Fit" : `${Math.round(view.scale * 100)}%`}
          </button>
          <button
            aria-label={`Zoom ${contentNoun} in`}
            className="attachment-preview-action"
            disabled={view.scale >= MAX_SCALE}
            onClick={() => zoomAt(view.scale + ZOOM_STEP)}
            type="button"
          >
            <Plus aria-hidden="true" size={16} />
          </button>
          {toolbarActions}
          {onClose ? (
            <button
              aria-label={`Close ${contentNoun} preview`}
              className="attachment-preview-close"
              onClick={onClose}
              type="button"
            >
              <X aria-hidden="true" size={18} />
            </button>
          ) : null}
        </div>
      </div>
      <div
        aria-label={`${image.label} zoomable preview`}
        className={[
          "attachment-preview-stage",
          view.scale !== FIT_SCALE ? "is-zoomed" : "",
          interacting ? "is-interacting" : "",
        ].filter(Boolean).join(" ")}
        onClick={onClick}
        onDoubleClick={onDoubleClick}
        onKeyDown={onKeyDown}
        onPointerCancel={onPointerEnd}
        onPointerDown={onPointerDown}
        onPointerMove={onPointerMove}
        onPointerUp={onPointerEnd}
        onWheel={onWheel}
        ref={stageRef}
        tabIndex={0}
      >
        <img
          alt={image.label}
          className={imageClassName}
          draggable={false}
          ref={imageRef}
          src={image.url}
          style={imageStyle}
        />
      </div>
    </>
  );
}

function constrainView(
  view: ImageView,
  stage: HTMLDivElement,
  image: HTMLImageElement | null,
): ImageView {
  if (!image) return view;
  const maxX = panLimit(image.offsetWidth * view.scale);
  const maxY = panLimit(image.offsetHeight * view.scale);
  return {
    ...view,
    x: clamp(view.x, -maxX, maxX),
    y: clamp(view.y, -maxY, maxY),
  };
}

function imageLayoutOrigin(
  stage: HTMLDivElement,
  image: HTMLImageElement | null,
  view: ImageView,
): PointerPoint {
  if (image) {
    const bounds = image.getBoundingClientRect();
    return {
      // The transformed image bounds include the current translation. Remove it
      // to recover the actual CSS transform origin used by the fitted image.
      x: bounds.left + bounds.width / 2 - view.x,
      y: bounds.top + bounds.height / 2 - view.y,
    };
  }
  const bounds = stage.getBoundingClientRect();
  return { x: bounds.left + bounds.width / 2, y: bounds.top + bounds.height / 2 };
}

/** Every zoom level uses the same recoverable edge-to-center pan boundary. */
function panLimit(contentSize: number) {
  return contentSize / 2;
}

function clamp(value: number, minimum: number, maximum: number) {
  return Math.min(maximum, Math.max(minimum, value));
}

type PointerPoint = { x: number; y: number };
type PointerPair = [PointerPoint, PointerPoint];

function pointerPair(pointers: Map<number, PointerPoint>): PointerPair | undefined {
  const points = Array.from(pointers.values());
  return points.length === 2 ? [points[0], points[1]] : undefined;
}

function pointerDistance([first, second]: PointerPair) {
  return Math.hypot(second.x - first.x, second.y - first.y);
}

function pointerMidpoint([first, second]: PointerPair, origin: PointerPoint): PointerPoint {
  return {
    x: (first.x + second.x) / 2 - origin.x,
    y: (first.y + second.y) / 2 - origin.y,
  };
}
