import { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import type { CSSProperties, KeyboardEvent, PointerEvent } from "react";
import { MoreHorizontal } from "lucide-react";
import type {
  UserMessageAnchor,
  UserMessageNavigation,
} from "./useTaskChatScroll";

type UserMessageNavigatorProps = {
  navigation: UserMessageNavigation;
};

type DragState = {
  pointerId: number;
  startScrollTop: number;
  startY: number;
};

type CurrentIndicatorPosition = {
  top: number;
  width: number;
};

type ScrollEdges = {
  end: boolean;
  start: boolean;
};

/** Navigates the loaded User-message projection without creating a second Chat model. */
export function UserMessageNavigator({
  navigation,
}: UserMessageNavigatorProps) {
  const { anchors, currentIndex, hasEarlier, pendingPrevious } = navigation;
  const [previewIndex, setPreviewIndex] = useState(currentIndex);
  const [currentIndicatorPosition, setCurrentIndicatorPosition] = useState<CurrentIndicatorPosition>();
  const [mobileExpanded, setMobileExpanded] = useState(false);
  const [scrollEdges, setScrollEdges] = useState<ScrollEdges>({ end: true, start: true });
  const [suppressFocusPresentation, setSuppressFocusPresentation] = useState(false);
  const navigatorRef = useRef<HTMLElement>(null);
  const navigatorScrollRef = useRef<HTMLDivElement>(null);
  const previewRef = useRef<HTMLDivElement>(null);
  const markerRefs = useRef<Array<HTMLButtonElement | null>>([]);
  const dragRef = useRef<DragState | undefined>(undefined);
  const didDragRef = useRef(false);
  const previewAnchor = anchors[previewIndex] ?? anchors[currentIndex] ?? anchors[0];

  const updateScrollEdges = useCallback(() => {
    const scroll = navigatorScrollRef.current;
    if (!scroll) return;
    const next = {
      end: scroll.scrollTop + scroll.clientHeight >= scroll.scrollHeight - 1,
      start: scroll.scrollTop <= 1,
    };
    setScrollEdges((current) => (
      current.end === next.end && current.start === next.start ? current : next
    ));
  }, []);

  const positionPreview = useCallback((index: number) => {
    const navigator = navigatorRef.current;
    const preview = previewRef.current;
    const marker = markerRefs.current[index];
    if (!navigator || !preview || !marker) return;

    const navigatorBounds = navigator.getBoundingClientRect();
    const previewBounds = preview.getBoundingClientRect();
    const markerBounds = marker.getBoundingClientRect();
    const markerCenter = markerBounds.top - navigatorBounds.top + markerBounds.height / 2;
    const previewHalf = previewBounds.height / 2;
    const edgeInset = 8;
    const minimumCenter = previewHalf + edgeInset;
    const maximumCenter = navigatorBounds.height - previewHalf - edgeInset;
    const previewCenter = minimumCenter <= maximumCenter
      ? Math.min(maximumCenter, Math.max(minimumCenter, markerCenter))
      : navigatorBounds.height / 2;
    const connectorPosition = markerCenter - previewCenter + previewHalf;

    navigator.style.setProperty("--user-message-preview-top", `${previewCenter}px`);
    preview.style.setProperty(
      "--user-message-preview-connector-top",
      `${Math.min(previewBounds.height - 10, Math.max(10, connectorPosition))}px`,
    );
  }, []);

  useEffect(() => {
    setPreviewIndex(currentIndex);
    markerRefs.current[currentIndex]?.scrollIntoView({ block: "center" });
  }, [currentIndex]);

  useLayoutEffect(() => {
    positionPreview(previewIndex);
  }, [anchors.length, positionPreview, previewIndex, previewAnchor?.text]);

  useLayoutEffect(() => {
    const marker = markerRefs.current[currentIndex];
    const anchor = anchors[currentIndex];
    if (!marker || !anchor) return;
    const next = {
      top: marker.offsetTop + (marker.offsetHeight - 2) / 2,
      width: messageMarkerWidth(anchor.text) * 1.1,
    };
    setCurrentIndicatorPosition((current) => (
      current?.top === next.top && current.width === next.width ? current : next
    ));
  }, [anchors, currentIndex]);

  useLayoutEffect(() => {
    updateScrollEdges();
  }, [anchors.length, mobileExpanded, updateScrollEdges]);

  useEffect(() => {
    if (typeof window === "undefined") return;
    const reposition = () => {
      positionPreview(previewIndex);
      updateScrollEdges();
    };
    window.addEventListener("resize", reposition);
    return () => window.removeEventListener("resize", reposition);
  }, [positionPreview, previewIndex, updateScrollEdges]);

  if (anchors.length === 0 || (anchors.length === 1 && !hasEarlier)) return null;

  const visiblePreviewAnchor = previewAnchor ?? anchors[0]!;
  const onKeyDown = (event: KeyboardEvent<HTMLElement>) => {
    if (event.altKey || event.ctrlKey || event.metaKey) return;
    if (event.key === "Escape" && mobileExpanded) {
      event.preventDefault();
      setMobileExpanded(false);
      setSuppressFocusPresentation(true);
      return;
    }
    setSuppressFocusPresentation(false);
    if (event.key === "ArrowUp" || event.key === "ArrowLeft") {
      event.preventDefault();
      navigation.goPrevious();
    } else if (event.key === "ArrowDown" || event.key === "ArrowRight") {
      event.preventDefault();
      navigation.goNext();
    } else if (event.key === "Home") {
      event.preventDefault();
      navigation.goFirst();
    } else if (event.key === "End") {
      event.preventDefault();
      navigation.goLast();
    }
  };
  const finishDrag = (event: PointerEvent<HTMLDivElement>) => {
    if (dragRef.current?.pointerId !== event.pointerId) return;
    dragRef.current = undefined;
    if (event.currentTarget.hasPointerCapture(event.pointerId)) {
      event.currentTarget.releasePointerCapture(event.pointerId);
    }
    window.setTimeout(() => { didDragRef.current = false; }, 0);
  };

  return (
    <nav
      aria-label="User message navigation"
      className="user-message-navigator"
      data-mobile-expanded={mobileExpanded ? "true" : undefined}
      data-suppress-focus-presentation={suppressFocusPresentation ? "true" : undefined}
      onBlurCapture={(event) => {
        if (!event.currentTarget.contains(event.relatedTarget as Node | null)) {
          setMobileExpanded(false);
        }
      }}
      onFocusCapture={() => setSuppressFocusPresentation(false)}
      onKeyDown={onKeyDown}
      onMouseLeave={() => {
        setPreviewIndex(currentIndex);
        setSuppressFocusPresentation(true);
      }}
      ref={navigatorRef}
    >
      <button
        aria-expanded={mobileExpanded}
        aria-label={mobileExpanded ? "Close user message navigation" : "Open user message navigation"}
        className="user-message-navigator-mobile-toggle"
        onClick={() => {
          setMobileExpanded((expanded) => !expanded);
          setSuppressFocusPresentation(true);
        }}
        onFocus={() => setSuppressFocusPresentation(true)}
        type="button"
      >
        <span aria-hidden="true"><span /><span /><span /></span>
      </button>
      <div className="user-message-navigator-preview" ref={previewRef}>
        <div className="user-message-navigator-preview-content" key={visiblePreviewAnchor.key}>
          <p>{messageFullPreview(visiblePreviewAnchor.text)}</p>
        </div>
      </div>
      <div
        className="user-message-navigator-scroll"
        data-at-scroll-end={scrollEdges.end ? "true" : undefined}
        data-at-scroll-start={scrollEdges.start ? "true" : undefined}
        onScroll={() => {
          positionPreview(previewIndex);
          updateScrollEdges();
        }}
        onPointerCancel={finishDrag}
        onPointerDown={(event) => {
          if (event.pointerType !== "mouse" || event.button !== 0) return;
          dragRef.current = {
            pointerId: event.pointerId,
            startScrollTop: event.currentTarget.scrollTop,
            startY: event.clientY,
          };
          didDragRef.current = false;
        }}
        onPointerMove={(event) => {
          if (dragRef.current?.pointerId !== event.pointerId) return;
          const distance = event.clientY - dragRef.current.startY;
          if (Math.abs(distance) < 3) return;
          didDragRef.current = true;
          if (!event.currentTarget.hasPointerCapture(event.pointerId)) {
            event.currentTarget.setPointerCapture(event.pointerId);
          }
          event.currentTarget.scrollTop = dragRef.current.startScrollTop - distance;
        }}
        onPointerUp={finishDrag}
        ref={navigatorScrollRef}
      >
        <div className="user-message-position-track">
          {currentIndicatorPosition ? (
            <span
              aria-hidden="true"
              className="user-message-current-indicator"
              style={{
                "--user-message-current-top": `${currentIndicatorPosition.top}px`,
                "--user-message-current-width": `${currentIndicatorPosition.width}px`,
              } as CSSProperties}
            />
          ) : null}
          {hasEarlier ? (
            <button
              aria-label={pendingPrevious ? "Loading earlier user messages" : "Load earlier user messages"}
              className="user-message-history-cap"
              disabled={pendingPrevious}
              onClick={navigation.goPrevious}
              type="button"
            >
              <MoreHorizontal aria-hidden="true" size={12} strokeWidth={1.5} />
            </button>
          ) : null}
          {anchors.map((anchor, index) => (
            <button
              aria-current={index === currentIndex ? "true" : undefined}
              aria-label={messageLabel(anchor, index, anchors.length, hasEarlier)}
              className="user-message-position-marker"
              data-preview={index === previewIndex ? "true" : undefined}
              key={anchor.key}
              onClick={(event) => {
                if (didDragRef.current) {
                  event.preventDefault();
                  return;
                }
                setPreviewIndex(index);
                navigation.navigateTo(anchor);
                setMobileExpanded(false);
                setSuppressFocusPresentation(true);
              }}
              onFocus={() => setPreviewIndex(index)}
              onMouseEnter={() => setPreviewIndex(index)}
              ref={(node) => { markerRefs.current[index] = node; }}
              style={{
                "--user-message-length": `${messageMarkerWidth(anchor.text)}px`,
              } as CSSProperties}
              type="button"
            >
              <span />
            </button>
          ))}
        </div>
      </div>

      <span aria-live="polite" className="visually-hidden" role="status">
        {navigation.announcement}
      </span>
    </nav>
  );
}

function messageLabel(
  anchor: UserMessageAnchor,
  index: number,
  count: number,
  hasEarlier: boolean,
) {
  const position = hasEarlier
    ? `${index + 1} of ${count} loaded`
    : `${index + 1} of ${count}`;
  return `User message ${position}: ${messagePreview(anchor.text)}`;
}

function messagePreview(text: string) {
  const normalized = text.replace(/\s+/g, " ").trim();
  if (!normalized) return "Attachment-only message";
  return normalized.length > 88 ? `${normalized.slice(0, 87)}…` : normalized;
}

function messageFullPreview(text: string) {
  return text.trim() || "Attachment-only message";
}

function messageMarkerWidth(text: string) {
  return Math.min(13, 4 + messageFullPreview(text).length / 18);
}
