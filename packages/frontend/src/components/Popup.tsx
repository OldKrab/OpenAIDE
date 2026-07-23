import {
  FloatingFocusManager,
  FloatingOverlay,
  FloatingPortal,
  autoUpdate,
  flip,
  offset,
  shift,
  size,
  useClick,
  useDismiss,
  useFloating,
  useInteractions,
  useRole,
  useTransitionStyles,
  type Placement,
} from "@floating-ui/react";
import {
  useEffect,
  useId,
  useLayoutEffect,
  type ButtonHTMLAttributes,
  type KeyboardEvent as ReactKeyboardEvent,
  type ReactNode,
  type RefCallback,
  type RefObject,
} from "react";

export type PopupTriggerProps = ButtonHTMLAttributes<HTMLButtonElement> & {
  "data-oa-popup-trigger"?: true;
  ref: RefCallback<HTMLButtonElement>;
};

type AnchoredPopupProps = {
  children: ReactNode;
  className?: string;
  label: string;
  onOpenChange: (open: boolean) => void;
  open: boolean;
  placement?: Placement;
  trigger: (props: PopupTriggerProps) => ReactNode;
};

/** Keeps top/bottom popups on their chosen side of the trigger. */
export function anchoredPopupMaxHeight({
  availableHeight,
  placement,
  referenceBottom,
  referenceTop,
  viewportHeight,
}: {
  availableHeight: number;
  placement: Placement;
  referenceBottom: number;
  referenceTop: number;
  viewportHeight: number;
}) {
  const viewportPadding = 8;
  const triggerGap = 4;
  const side = placement.split("-")[0];
  const sideHeight = side === "top"
    ? referenceTop - viewportPadding - triggerGap
    : side === "bottom"
      ? viewportHeight - referenceBottom - viewportPadding - triggerGap
      : availableHeight;
  return Math.max(0, Math.min(availableHeight, sideHeight));
}

/** Anchored action surface with one owner for placement, dismissal, and focus return. */
export function PopupMenu({
  ...props
}: AnchoredPopupProps) {
  const browser = typeof document !== "undefined"
    && Boolean(document.body)
    && typeof Element !== "undefined";
  return browser
    ? <BrowserAnchoredPopup {...props} surfaceRole="menu" />
    : <FallbackAnchoredPopup {...props} surfaceRole="menu" />;
}

/** Anchored composite surface for forms or controls that cannot use menu semantics. */
export function PopupPanel({ ...props }: AnchoredPopupProps) {
  const browser = typeof document !== "undefined"
    && Boolean(document.body)
    && typeof Element !== "undefined";
  return browser
    ? <BrowserAnchoredPopup {...props} surfaceRole="dialog" />
    : <FallbackAnchoredPopup {...props} surfaceRole="dialog" />;
}

/** Completion listbox whose editor keeps DOM focus and owns selection state. */
export function EditorListbox({
  children,
  className,
  id,
  label,
}: {
  children: ReactNode;
  className?: string;
  id: string;
  label: string;
}) {
  return (
    <div
      aria-label={label}
      className={["oa-popup-surface", "oa-popup-listbox", className].filter(Boolean).join(" ")}
      id={id}
      role="listbox"
    >
      {children}
    </div>
  );
}

type HoverSurfaceProps = {
  anchor: HTMLElement;
  children: ReactNode;
  className?: string;
  containerRef?: RefObject<HTMLDivElement | null>;
  dataKind?: string;
  label?: string;
  onPointerEnter?: () => void;
  onPointerLeave?: () => void;
  placement?: Placement;
  semanticRole: "dialog" | "tooltip";
};

/** Portal-positioned hover surface; its caller owns hover intent and content state. */
export function PopupHoverSurface(props: HoverSurfaceProps) {
  const browser = typeof document !== "undefined"
    && Boolean(document.body)
    && typeof Element !== "undefined";
  if (!browser) {
    return (
      <div
        aria-label={props.label}
        className={["oa-popup-surface", "oa-popup-hover", props.className].filter(Boolean).join(" ")}
        data-reference-kind={props.dataKind}
        onPointerEnter={props.onPointerEnter}
        onPointerLeave={props.onPointerLeave}
        ref={props.containerRef}
        role={props.semanticRole}
      >
        {props.children}
      </div>
    );
  }
  return <BrowserHoverSurface {...props} />;
}

/** Modal layer with shared focus trapping, scroll locking, and top-layer dismissal. */
export function PopupDialog({
  backdropClassName,
  children,
  className,
  label,
  onOpenChange,
  open,
}: {
  backdropClassName?: string;
  children: ReactNode;
  className?: string;
  label: string;
  onOpenChange: (open: boolean) => void;
  open: boolean;
}) {
  const browser = typeof document !== "undefined"
    && Boolean(document.body)
    && typeof Element !== "undefined";
  if (!open) return null;
  if (!browser) {
    return (
      <div className={["oa-popup-backdrop", backdropClassName].filter(Boolean).join(" ")}>
        <div aria-label={label} aria-modal="true" className={className} role="dialog">{children}</div>
      </div>
    );
  }
  return (
    <BrowserPopupDialog
      backdropClassName={backdropClassName}
      className={className}
      label={label}
      onOpenChange={onOpenChange}
    >
      {children}
    </BrowserPopupDialog>
  );
}

function BrowserPopupDialog({
  backdropClassName,
  children,
  className,
  label,
  onOpenChange,
}: {
  backdropClassName?: string;
  children: ReactNode;
  className?: string;
  label: string;
  onOpenChange: (open: boolean) => void;
}) {
  const { context, refs } = useFloating({ onOpenChange, open: true });
  const dismiss = useDismiss(context, {
    outsidePress: (event) => {
      const target = event.target;
      return !(target instanceof Element && target.closest("[data-oa-popup-trigger]"));
    },
    outsidePressEvent: "pointerdown",
  });
  const role = useRole(context, { role: "dialog" });
  const { getFloatingProps } = useInteractions([dismiss, role]);
  return (
    <FloatingPortal id="openaide-popup-layer">
      <FloatingOverlay className={["oa-popup-backdrop", backdropClassName].filter(Boolean).join(" ")} lockScroll>
        <FloatingFocusManager context={context} modal returnFocus>
          <div
            {...getFloatingProps({
              "aria-label": label,
              "aria-modal": true,
              className: ["oa-popup-dialog", className].filter(Boolean).join(" "),
              ref: refs.setFloating,
            })}
          >
            {children}
          </div>
        </FloatingFocusManager>
      </FloatingOverlay>
    </FloatingPortal>
  );
}

function BrowserHoverSurface({
  anchor,
  children,
  className,
  containerRef,
  dataKind,
  label,
  onPointerEnter,
  onPointerLeave,
  placement = "right-start",
  semanticRole,
}: HoverSurfaceProps) {
  const { floatingStyles, refs } = useFloating({
    middleware: [
      offset(8),
      flip({ padding: 8 }),
      shift({ padding: 8 }),
      size({
        padding: 8,
        apply({ availableHeight, elements }) {
          // Short editor panes keep the surface reachable instead of clipping it.
          elements.floating.style.maxHeight = `${Math.max(80, availableHeight)}px`;
        },
      }),
    ],
    placement,
    strategy: "fixed",
    whileElementsMounted: autoUpdate,
  });
  useLayoutEffect(() => {
    refs.setReference(anchor);
    return () => refs.setReference(null);
  }, [anchor, refs]);
  return (
    <FloatingPortal id="openaide-popup-layer">
      <div
        aria-label={label}
        className={["oa-popup-surface", "oa-popup-hover", className].filter(Boolean).join(" ")}
        data-reference-kind={dataKind}
        onPointerEnter={onPointerEnter}
        onPointerLeave={onPointerLeave}
        ref={(node) => {
          refs.setFloating(node);
          if (containerRef) containerRef.current = node;
        }}
        role={semanticRole}
        style={floatingStyles}
      >
        {children}
      </div>
    </FloatingPortal>
  );
}

function BrowserAnchoredPopup({
  children,
  className,
  label,
  onOpenChange,
  open,
  placement = "bottom-end",
  surfaceRole,
  trigger,
}: AnchoredPopupProps & { surfaceRole: "dialog" | "menu" }) {
  const floatingId = useId();
  const { context, floatingStyles, refs } = useFloating({
    middleware: [
      offset(4),
      // Preserve the trigger's alignment while changing sides; shift only the
      // distance required to keep a wider detail panel inside the viewport.
      flip({ flipAlignment: false, padding: 8 }),
      shift({ crossAxis: true, padding: 8 }),
      size({
        padding: 8,
        apply({ availableHeight, elements, placement: resolvedPlacement, rects }) {
          elements.floating.style.setProperty("--oa-popup-available-height", `${anchoredPopupMaxHeight({
            availableHeight,
            placement: resolvedPlacement,
            referenceBottom: rects.reference.y + rects.reference.height,
            referenceTop: rects.reference.y,
            viewportHeight: window.innerHeight,
          })}px`);
        },
      }),
    ],
    onOpenChange,
    open,
    placement,
    strategy: "fixed",
    whileElementsMounted: autoUpdate,
  });
  const click = useClick(context);
  const dismiss = useDismiss(context, { outsidePressEvent: "pointerdown" });
  const role = useRole(context, { role: surfaceRole });
  const { getFloatingProps, getReferenceProps } = useInteractions([click, dismiss, role]);
  const transition = useTransitionStyles(context, {
    close: { opacity: 0 },
    duration: { close: 45, open: 45 },
    initial: { opacity: 0 },
    open: { opacity: 1 },
  });
  const referenceProps = {
    ...getReferenceProps({
    "aria-controls": open ? floatingId : undefined,
    "aria-expanded": open,
    }),
    "data-oa-popup-trigger": true as const,
  } as ButtonHTMLAttributes<HTMLButtonElement> & { "data-oa-popup-trigger": true };
  const surface = (
    <div
      {...getFloatingProps({
        "aria-label": label,
        className: [
          "oa-popup-surface",
          surfaceRole === "menu" ? "oa-popup-menu" : "oa-popup-panel",
          className,
        ].filter(Boolean).join(" "),
        id: floatingId,
        ref: refs.setFloating,
        style: { ...floatingStyles, ...transition.styles },
      })}
      aria-labelledby={undefined}
      onKeyDown={surfaceRole === "menu" ? moveMenuFocus : undefined}
    >
      {children}
    </div>
  );
  return (
    <>
      {trigger({
        ...referenceProps,
        ref: refs.setReference as RefCallback<HTMLButtonElement>,
      })}
      {transition.isMounted ? (
        <FloatingPortal id="openaide-popup-layer">
          <FloatingFocusManager context={context} modal={false}>
            {surface}
          </FloatingFocusManager>
        </FloatingPortal>
      ) : null}
    </>
  );
}

function FallbackAnchoredPopup({
  children,
  className,
  label,
  onOpenChange,
  open,
  surfaceRole,
  trigger,
}: AnchoredPopupProps & { surfaceRole: "dialog" | "menu" }) {
  const floatingId = useId();
  useEffect(() => {
    if (!open || typeof document === "undefined") return;
    const onPointerDown = (event: globalThis.PointerEvent) => {
      const target = event.target as { closest?: (selector: string) => Element | null } | null;
      if (target?.closest?.(
        ".oa-popup-surface, .composer-menu-anchor, .composer-option-anchor, .new-task-context-anchor",
      )) return;
      onOpenChange(false);
    };
    const onKeyDown = (event: globalThis.KeyboardEvent) => {
      if (event.key === "Escape") onOpenChange(false);
    };
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [onOpenChange, open]);
  return (
    <>
      {trigger({
        "aria-controls": open ? floatingId : undefined,
        "aria-expanded": open,
        "aria-haspopup": surfaceRole,
        "data-oa-popup-trigger": true,
        onClick: () => onOpenChange(!open),
        ref: () => undefined,
      })}
      {open ? (
        <div
          aria-label={label}
          className={[
            "oa-popup-surface",
            surfaceRole === "menu" ? "oa-popup-menu" : "oa-popup-panel",
            className,
          ].filter(Boolean).join(" ")}
          id={floatingId}
          role={surfaceRole}
        >
          {children}
        </div>
      ) : null}
    </>
  );
}

function moveMenuFocus(event: ReactKeyboardEvent<HTMLDivElement>) {
  if (!["ArrowDown", "ArrowUp", "Home", "End"].includes(event.key)) return;
  const items = Array.from(
    event.currentTarget.querySelectorAll<HTMLElement>(
      '[role^="menuitem"]:not([disabled]):not([aria-disabled="true"])',
    ),
  );
  if (!items.length) return;
  event.preventDefault();
  event.stopPropagation();
  const currentIndex = items.indexOf(document.activeElement as HTMLElement);
  if (event.key === "Home") {
    items[0].focus();
    return;
  }
  if (event.key === "End") {
    items[items.length - 1].focus();
    return;
  }
  const direction = event.key === "ArrowDown" ? 1 : -1;
  const fallbackIndex = direction === 1 ? -1 : 0;
  items[(currentIndex === -1 ? fallbackIndex : currentIndex + direction + items.length) % items.length].focus();
}
