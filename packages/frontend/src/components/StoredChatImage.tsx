import { ImageOff } from "lucide-react";
import { createContext, useContext, useEffect, useMemo, useRef, useState, type CSSProperties, type ReactNode } from "react";
import { chatImageCache, type ChatImageCache } from "../state/chatImageCache";

/** A stored Chat image as a Chat message names it (ADR-0075): no bytes, only identity. */
export type StoredChatImageReference = {
  imageId: string;
  mediaType: string;
  sizeBytes?: number;
  /** Display size in pixels, when App Server could read it from the image. */
  width?: number;
  height?: number;
};

/** The `width` and `height` an `<img>` or placeholder reserves, or nothing when unknown. */
export function storedChatImageSize(reference: StoredChatImageReference) {
  const { width, height } = reference;
  if (!width || !height) return undefined;
  return { width, height };
}

type ChatImageSource = {
  cache: ChatImageCache;
  load(imageId: string): Promise<string>;
  taskId?: string;
};

const unavailableSource: ChatImageSource = {
  cache: chatImageCache,
  load: () => Promise.reject(new Error("Chat images are unavailable here.")),
};

const ChatImageContext = createContext<ChatImageSource>(unavailableSource);

/**
 * Lets every Chat row of one Task (its Chat and its Subagent histories, which share the
 * Task's images) load stored images through the Frontend intent layer, so rendering
 * components never call the App Server Protocol themselves.
 */
export function ChatImageProvider({
  cache = chatImageCache,
  children,
  loadImage,
  taskId,
}: {
  cache?: ChatImageCache;
  children: ReactNode;
  loadImage?: (taskId: string, imageId: string) => Promise<string>;
  taskId: string;
}) {
  // The loader is read at call time so a new callback identity does not restart loads.
  const loadImageRef = useRef(loadImage);
  loadImageRef.current = loadImage;
  const source = useMemo<ChatImageSource>(() => ({
    cache,
    load: (imageId) => loadImageRef.current
      ? loadImageRef.current(taskId, imageId)
      : unavailableSource.load(imageId),
    taskId,
  }), [cache, taskId]);
  return <ChatImageContext.Provider value={source}>{children}</ChatImageContext.Provider>;
}

export type StoredChatImageState =
  | { status: "loading" }
  | { status: "ready"; url: string }
  | { status: "failed"; retry?: () => void };

type Loaded = { key: string; data?: string; failed?: boolean };

/** Loads a stored image through the shared cache; the result is a data URL for an `<img>`. */
export function useStoredChatImage(reference: StoredChatImageReference): StoredChatImageState {
  const { cache, load, taskId = "" } = useContext(ChatImageContext);
  const { imageId, mediaType } = reference;
  const renderable = mediaType.startsWith("image/");
  const key = `${taskId}\u0000${imageId}`;
  const [loaded, setLoaded] = useState<Loaded>(() => ({ key, data: cache.get(imageId) }));
  // A reused view adopts the new image at once, from memory when it is cached.
  const current = loaded.key === key ? loaded : { key, data: cache.get(imageId) };
  const needsLoad = renderable && current.data === undefined && !current.failed;

  useEffect(() => {
    if (!needsLoad) return;
    let active = true;
    cache.load(taskId, imageId, () => load(imageId)).then(
      (data) => { if (active) setLoaded({ key, data }); },
      () => { if (active) setLoaded({ key, failed: true }); },
    );
    return () => { active = false; };
  }, [cache, imageId, key, load, needsLoad, taskId]);

  const data = current.data;
  const url = useMemo(() => data === undefined ? undefined : `data:${mediaType};base64,${data}`, [data, mediaType]);
  if (!renderable) return { status: "failed" };
  if (url) return { status: "ready", url };
  if (current.failed) return { status: "failed", retry: () => setLoaded({ key }) };
  return { status: "loading" };
}

/**
 * The loading and unavailable states of a stored image, in the frame its loaded image
 * uses, so the rest of the message renders normally and the row keeps its footprint.
 * With the image's size known, the frame holds a spacer shaped like the loaded image,
 * so the Chat does not shift when the bytes arrive; without it, one placeholder height.
 */
export function StoredChatImagePlaceholder({
  className,
  label,
  size,
  state,
}: {
  className: string;
  label: string;
  size?: { width: number; height: number };
  state: Exclude<StoredChatImageState, { status: "ready" }>;
}) {
  const space = size ? (
    <span
      aria-hidden="true"
      className="chat-stored-image-space"
      style={{ aspectRatio: `${size.width} / ${size.height}`, "--chat-image-width": `${size.width}px` } as CSSProperties}
    />
  ) : null;
  if (state.status === "loading") {
    return (
      <span
        aria-busy="true"
        aria-label={`Loading ${label}`}
        className={`${className} chat-stored-image`}
        data-image-sized={size ? "" : undefined}
        data-image-state="loading"
        role="img"
      >
        {space}
      </span>
    );
  }
  return (
    <span
      aria-label={`${label} unavailable`}
      className={`${className} chat-stored-image`}
      data-image-sized={size ? "" : undefined}
      data-image-state="failed"
      role="group"
    >
      {space}
      <span className="chat-stored-image-status">
        <ImageOff aria-hidden="true" size={16} />
        <span>Image unavailable</span>
        {state.retry ? (
          <button
            aria-label={`Retry loading ${label}`}
            className="chat-stored-image-retry"
            onClick={state.retry}
            type="button"
          >
            Retry
          </button>
        ) : null}
      </span>
    </span>
  );
}
