import {
  Fragment,
  createContext,
  isValidElement,
  memo,
  useContext,
  useEffect,
  useRef,
  useState,
  type ComponentProps,
  type ReactNode,
} from "react";
import { Check, CircleAlert, Copy } from "lucide-react";
import Markdown, { defaultUrlTransform, type Components, type ExtraProps } from "react-markdown";
import remarkGfm from "remark-gfm";
import { postHostMessage } from "../services/hostBridge";
import {
  chatMarkdownFileLocation,
  markdownFileLocation,
  pathLikeFileLocation,
  relativeMarkdownHref,
} from "../state/fileViewerReferences";
import { useAgentFileOpen, type OpenAgentFileReference } from "./agentFileOpen";
import { copyText } from "./clipboard";
import { MermaidDiagram } from "./MermaidDiagram";

type AgentMarkdownProps = {
  className?: string;
  /** Resolves relative hrefs from an already-open File Tab without sending a raw path. */
  onOpenRelativeHref?: (href: string) => void;
  quoteSource?: "agent";
  renderDiagrams?: boolean;
  streaming?: boolean;
  text: string;
};

const MarkdownLinkBehaviorContext = createContext<{
  onOpenRelativeHref?: (href: string) => void;
  openFile?: OpenAgentFileReference;
}>({});

// Unrelated Task and composer updates must not re-enter the synchronous Markdown parser.
export const AgentMarkdown = memo(function AgentMarkdown({
  className,
  onOpenRelativeHref,
  quoteSource,
  renderDiagrams = false,
  streaming = false,
  text,
}: AgentMarkdownProps) {
  const openFile = useAgentFileOpen();
  const parts = splitDataImageMarkdown(text);
  return (
    <MarkdownLinkBehaviorContext.Provider value={{ onOpenRelativeHref, openFile }}>
      <div className={className} data-quote-source={quoteSource}>
        {parts.map((part, index) => (
          <Fragment key={index}>
            {part.kind === "image" ? (
              <>
                <AgentMarkdownImage label={part.label} url={part.url} />
                {streaming && index === parts.length - 1 ? <StreamingCaret /> : null}
              </>
            ) : (
              <MarkdownRenderer
                renderDiagrams={renderDiagrams}
                streaming={streaming && index === parts.length - 1}
                text={part.text}
              />
            )}
          </Fragment>
        ))}
      </div>
    </MarkdownLinkBehaviorContext.Provider>
  );
});

function MarkdownRenderer({ renderDiagrams, streaming, text }: { renderDiagrams: boolean; streaming: boolean; text: string }) {
  if (!text) return streaming ? <StreamingCaret /> : null;
  return (
    <MarkdownSourceContext.Provider value={{ renderDiagrams, source: text, streaming }}>
      <Markdown
        rehypePlugins={streaming ? [appendStreamingCaret] : []}
        remarkPlugins={[remarkGfm]}
        skipHtml
        urlTransform={safeMarkdownUrl}
        components={agentMarkdownComponents}
      >
        {text}
      </Markdown>
    </MarkdownSourceContext.Provider>
  );
}

const MarkdownSourceContext = createContext({ renderDiagrams: false, source: "", streaming: false });

// Stable component identities keep native pointer gestures alive across streamed Markdown updates.
const agentMarkdownComponents: Components = {
  a: AgentMarkdownAnchor,
  code: AgentMarkdownCode,
  pre: MarkdownPreBlock,
  blockquote: MarkdownQuoteBlock,
};

function AgentMarkdownAnchor({ children, href, node: _node, ...props }: ComponentProps<"a"> & ExtraProps) {
  const { onOpenRelativeHref, openFile } = useContext(MarkdownLinkBehaviorContext);
  const label = plainText(children) || "Image";
  if (isSafeDataImageUrl(href)) {
    return <AgentMarkdownImage label={label} url={href} />;
  }
  const relativeHref = relativeMarkdownHref(href);
  if (onOpenRelativeHref && relativeHref) {
    return (
      <a
        {...props}
        href={href}
        onClick={(event) => {
          event.preventDefault();
          onOpenRelativeHref(relativeHref);
        }}
      >
        {children}
      </a>
    );
  }
  const fileLocation = chatMarkdownFileLocation(href);
  if (fileLocation) {
    return (
      <a
        {...props}
        href={href}
        onClick={(event) => {
          event.preventDefault();
          activateAgentFileReference(openFile, fileLocation.path, fileLocation.line);
        }}
      >
        {children}
      </a>
    );
  }
  return href ? (
    <a {...props} href={href} rel="noreferrer" target="_blank">
      {children}
    </a>
  ) : (
    <span>{children}</span>
  );
}

function AgentMarkdownCode({ children, className, node: _node, ...props }: ComponentProps<"code"> & ExtraProps) {
  const { openFile } = useContext(MarkdownLinkBehaviorContext);
  const fenced = className?.split(/\s+/).some((token) => token.startsWith("language-"));
  const text = plainText(children);
  const location = !fenced ? pathLikeFileLocation(text) : undefined;
  if (!location) {
    return <code {...props} className={className}>{children}</code>;
  }
  return (
    <code
      {...props}
      className={["agent-file-ref", className].filter(Boolean).join(" ")}
      onClick={(event) => {
        event.preventDefault();
        activateAgentFileReference(openFile, location.path, location.line);
      }}
      onKeyDown={(event) => {
        if (event.key !== "Enter" && event.key !== " ") return;
        event.preventDefault();
        activateAgentFileReference(openFile, location.path, location.line);
      }}
      role="link"
      tabIndex={0}
    >
      {children}
    </code>
  );
}

function activateAgentFileReference(
  openFile: ReturnType<typeof useAgentFileOpen>,
  path: string,
  line?: number,
) {
  if (openFile) {
    openFile(path, line);
    return;
  }
  postHostMessage({ type: "tool.openPath", payload: { path, line } });
}

function MarkdownPreBlock({ children, node: _node, ...props }: ComponentProps<"pre"> & ExtraProps) {
  const context = useContext(MarkdownSourceContext);
  const text = plainText(children).replace(/\n$/, "");
  if (context.renderDiagrams && !context.streaming && isMermaidCode(children)) {
    return <MermaidDiagram source={text} />;
  }
  return (
    <MarkdownCopyBlock kind="code" text={text}>
      <pre {...props}>{children}</pre>
    </MarkdownCopyBlock>
  );
}

function MarkdownQuoteBlock({ children, node, ...props }: ComponentProps<"blockquote"> & ExtraProps) {
  const { source } = useContext(MarkdownSourceContext);
  const start = node?.position?.start.offset;
  const end = node?.position?.end.offset;
  // Rendered children have already lost Markdown markers, so copy from the source span and remove only this quote level.
  const text = typeof start === "number" && typeof end === "number"
    ? source.slice(start, end).replace(/^ {0,3}> ?/gm, "").trim()
    : markdownPlainText(children);
  return (
    <MarkdownCopyBlock kind="quote" text={text}>
      <blockquote {...props}>{children}</blockquote>
    </MarkdownCopyBlock>
  );
}

function isMermaidCode(children: ReactNode) {
  if (!isValidElement<{ className?: string }>(children)) return false;
  return children.props.className?.split(/\s+/).includes("language-mermaid") === true;
}

type CopyState = "idle" | "copied" | "failed";

function MarkdownCopyBlock({ children, kind, text }: { children: ReactNode; kind: "code" | "quote"; text: string }) {
  const [copyState, setCopyState] = useState<CopyState>("idle");
  const resetTimer = useRef<ReturnType<typeof setTimeout> | undefined>(undefined);
  useEffect(() => () => clearTimeout(resetTimer.current), []);
  const capitalizedKind = kind === "code" ? "Code" : "Quote";
  const feedback = copyState === "copied"
    ? { label: `${capitalizedKind} copied`, text: "Copied", title: "Copied" }
    : copyState === "failed"
      ? { label: `Copy ${kind} failed`, text: "Copy failed", title: "Copy failed" }
      : { label: `Copy ${kind}`, text: "", title: `Copy ${kind}` };
  const Icon = copyState === "copied" ? Check : copyState === "failed" ? CircleAlert : Copy;

  return (
    <div className={`agent-markdown-copy-block agent-markdown-${kind}-block`} data-copy-state={copyState}>
      <button
        aria-label={feedback.label}
        className={`agent-markdown-copy agent-markdown-${kind}-copy`}
        onClick={async () => {
          clearTimeout(resetTimer.current);
          try {
            await copyText(text);
            setCopyState("copied");
          } catch (error) {
            console.warn(`Failed to copy a Markdown ${kind} block.`, {
              error_kind: error instanceof Error && error.name ? error.name : typeof error,
            });
            setCopyState("failed");
          }
          resetTimer.current = setTimeout(() => setCopyState("idle"), 1_400);
        }}
        title={feedback.title}
        type="button"
      >
        <Icon aria-hidden="true" size={13} />
        {feedback.text ? <span aria-live="polite">{feedback.text}</span> : null}
      </button>
      {children}
    </div>
  );
}

type MarkdownTreeNode = {
  children?: MarkdownTreeNode[];
  properties?: Record<string, unknown>;
  tagName?: string;
  type: string;
  value?: string;
};

const atomicInlineTags = new Set(["a", "code", "del", "em", "img", "strong"]);

// Keep the caret at the visual end without inheriting the final inline element's styling.
function appendStreamingCaret() {
  return (tree: MarkdownTreeNode) => {
    const insertion = finalCaretInsertion(tree);
    if (!insertion) return;
    insertion.parent.children?.splice(insertion.index, 0, streamingCaretNode());
  };
}

function finalCaretInsertion(parent: MarkdownTreeNode): { parent: MarkdownTreeNode; index: number } | undefined {
  const children = parent.children ?? [];
  for (let index = children.length - 1; index >= 0; index -= 1) {
    const child = children[index];
    if (!child || !hasVisibleContent(child)) continue;
    if (child.type === "text" || (child.type === "element" && child.tagName && atomicInlineTags.has(child.tagName))) {
      return { parent, index: index + 1 };
    }
    const nested = finalCaretInsertion(child);
    if (nested) return nested;
    return { parent, index: index + 1 };
  }
  return undefined;
}

function hasVisibleContent(node: MarkdownTreeNode): boolean {
  if (node.type === "text") return Boolean(node.value?.trim());
  if (node.type === "element" && node.tagName === "img") return true;
  return node.children?.some(hasVisibleContent) ?? false;
}

function streamingCaretNode(): MarkdownTreeNode {
  return {
    type: "element",
    tagName: "span",
    properties: { ariaHidden: "true", className: ["chat-streaming-caret"] },
    children: [],
  };
}

function StreamingCaret() {
  return <span aria-hidden="true" className="chat-streaming-caret" />;
}

function AgentMarkdownImage({ label, url }: { label: string; url: string }) {
  return (
    <span className="agent-markdown-image-link">
      <img alt={label} src={url} />
      <span>{label}</span>
    </span>
  );
}

export type MarkdownPart = { kind: "markdown"; text: string } | { kind: "image"; label: string; url: string };

export function splitDataImageMarkdown(text: string): MarkdownPart[] {
  const parts: MarkdownPart[] = [];
  const imageLinkPattern = /\[([^\]\n]{1,120})\]\((data:image\/[a-z0-9.+-]+;base64,[a-z0-9+/=\s]+)\)/gi;
  let lastIndex = 0;
  for (const match of text.matchAll(imageLinkPattern)) {
    const index = match.index ?? 0;
    const label = match[1]?.trim() || "Image";
    const url = match[2]?.replace(/\s/g, "");
    if (!url || !isSafeDataImageUrl(url)) continue;
    if (index > lastIndex) parts.push({ kind: "markdown", text: text.slice(lastIndex, index) });
    parts.push({ kind: "image", label, url });
    lastIndex = index + match[0].length;
  }
  if (lastIndex < text.length) parts.push({ kind: "markdown", text: text.slice(lastIndex) });
  return parts.length ? parts : [{ kind: "markdown", text }];
}

function safeMarkdownUrl(value: string) {
  return isSafeDataImageUrl(value) || markdownFileLocation(value) || relativeMarkdownHref(value)
    ? value
    : defaultUrlTransform(value);
}

function isSafeDataImageUrl(value: unknown): value is string {
  return typeof value === "string" && /^data:image\/[a-z0-9.+-]+;base64,[a-z0-9+/=\s]+$/i.test(value);
}

function plainText(children: ReactNode): string {
  if (typeof children === "string" || typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(plainText).join("");
  if (isValidElement<{ children?: ReactNode }>(children)) return plainText(children.props.children);
  return "";
}

const markdownBlockTags = new Set(["blockquote", "li", "ol", "p", "pre", "ul"]);

function markdownPlainText(children: ReactNode): string {
  return markdownTextWithBreaks(children).replace(/\n{3,}/g, "\n\n").trim();
}

function markdownTextWithBreaks(children: ReactNode): string {
  if (typeof children === "string" || typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(markdownTextWithBreaks).join("");
  if (!isValidElement<{ children?: ReactNode }>(children)) return "";
  const text = markdownTextWithBreaks(children.props.children);
  return typeof children.type === "string" && markdownBlockTags.has(children.type) ? `${text}\n\n` : text;
}
