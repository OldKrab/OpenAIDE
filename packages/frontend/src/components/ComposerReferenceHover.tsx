import { FileText, ScanSearch } from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type { RefObject } from "react";
import { PopupHoverSurface } from "./Popup";

type ReferenceRect = {
  bottom: number;
  left: number;
  top: number;
};

type PopupSize = {
  height: number;
  width: number;
};

type ViewportSize = {
  height: number;
  width: number;
};

export type ComposerReferenceHoverModel = {
  description: string;
  kind: "command" | "file";
  label: string;
  type: string;
};

export function referenceHoverModelFromElement(element: HTMLElement): ComposerReferenceHoverModel | undefined {
  const {
    referenceDescription: description,
    referenceKind: kind,
    referenceLabel: label,
    referenceType: type,
  } = element.dataset;
  if ((kind !== "command" && kind !== "file") || !description || !label || !type) return undefined;
  return { description, kind, label, type };
}

/** Places quick info beside its token and keeps the surface inside the viewport. */
export function referenceHoverPosition(
  anchor: ReferenceRect,
  popup: PopupSize,
  viewport: ViewportSize,
) {
  const margin = 12;
  const gap = 8;
  const maximumLeft = viewport.width - margin - popup.width;
  const left = Math.max(margin, Math.min(anchor.left, maximumLeft));
  const below = anchor.bottom + gap;
  const above = anchor.top - gap - popup.height;
  const top = below + popup.height <= viewport.height - margin || above < margin ? below : above;
  return { left, top };
}

type HoverTarget = {
  anchor: HTMLElement;
  model: ComposerReferenceHoverModel;
};

type ReferenceHoverLayerProps = {
  contentKey?: string;
  rootRef: RefObject<HTMLElement | null>;
};

const OPEN_DELAY_MS = 300;

/** Delegates reference hover without adding state to the text surface that owns the tokens. */
export function ReferenceHoverLayer({
  contentKey,
  rootRef,
}: ReferenceHoverLayerProps) {
  const [target, setTarget] = useState<HoverTarget>();
  const activeTargetRef = useRef<HoverTarget | undefined>(undefined);
  const pendingAnchorRef = useRef<HTMLElement | undefined>(undefined);
  const openTimerRef = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);

  useEffect(() => {
    activeTargetRef.current = target;
  }, [target]);

  useEffect(() => {
    clearTimeout(openTimerRef.current);
    pendingAnchorRef.current = undefined;
    activeTargetRef.current = undefined;
    setTarget(undefined);
  }, [contentKey]);

  useEffect(() => {
    const root = rootRef.current;
    // Renderer tests use minimal element refs; quick info only exists in a browser DOM.
    if (!root || typeof root.addEventListener !== "function") return;

    const referenceFromEventTarget = (eventTarget: EventTarget | null) => {
      if (!(eventTarget instanceof Element)) return undefined;
      const reference = eventTarget.closest<HTMLElement>("[data-reference-kind]");
      return reference && root.contains(reference) ? reference : undefined;
    };
    const clearPendingOpen = () => {
      clearTimeout(openTimerRef.current);
      openTimerRef.current = undefined;
      pendingAnchorRef.current = undefined;
    };
    const closeReference = (reference: HTMLElement | undefined) => {
      clearPendingOpen();
      if (!reference || activeTargetRef.current?.anchor === reference) {
        activeTargetRef.current = undefined;
        setTarget(undefined);
      }
    };
    const handlePointerOver = (event: PointerEvent) => {
      const anchor = referenceFromEventTarget(event.target);
      if (!anchor
        || pendingAnchorRef.current === anchor
        || activeTargetRef.current?.anchor === anchor) return;

      clearPendingOpen();
      const model = referenceHoverModelFromElement(anchor);
      if (!model) return;
      pendingAnchorRef.current = anchor;
      openTimerRef.current = setTimeout(() => {
        pendingAnchorRef.current = undefined;
        if (!anchor.isConnected) return;
        const nextTarget = { anchor, model };
        activeTargetRef.current = nextTarget;
        setTarget(nextTarget);
      }, OPEN_DELAY_MS);
    };
    const handlePointerOut = (event: PointerEvent) => {
      const reference = referenceFromEventTarget(event.target);
      if (!reference || referenceFromEventTarget(event.relatedTarget) === reference) return;
      closeReference(reference);
    };
    const handlePointerLeave = () => closeReference(undefined);

    root.addEventListener("pointerover", handlePointerOver);
    root.addEventListener("pointerout", handlePointerOut);
    root.addEventListener("pointerleave", handlePointerLeave);
    return () => {
      clearPendingOpen();
      root.removeEventListener("pointerover", handlePointerOver);
      root.removeEventListener("pointerout", handlePointerOut);
      root.removeEventListener("pointerleave", handlePointerLeave);
    };
  }, [rootRef]);

  return target ? <ComposerReferenceHover target={target} /> : null;
}

function ComposerReferenceHover({ target }: { target: HoverTarget }) {
  const Icon = target.model.kind === "command" ? ScanSearch : FileText;
  return (
    <PopupHoverSurface
      anchor={target.anchor}
      className="composer-reference-hover"
      dataKind={target.model.kind}
      placement="bottom-start"
      semanticRole="tooltip"
    >
      <Icon aria-hidden="true" className="composer-reference-hover-icon" size={16} strokeWidth={1.7} />
      <div className="composer-reference-hover-content">
        <div className="composer-reference-hover-heading">
          <strong>{target.model.label}</strong>
          <span>{target.model.type}</span>
        </div>
        <p>{target.model.description}</p>
      </div>
    </PopupHoverSurface>
  );
}
