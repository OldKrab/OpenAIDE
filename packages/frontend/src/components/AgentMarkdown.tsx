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
import { copyText } from "./clipboard";

type AgentMarkdownProps = {
  className?: string;
  quoteSource?: "agent";
  streaming?: boolean;
  text: string;
};

// Unrelated Task and composer updates must not re-enter the synchronous Markdown parser.
export const AgentMarkdown = memo(function AgentMarkdown({ className, quoteSource, streaming = false, text }: AgentMarkdownProps) {
  const parts = splitDataImageMarkdown(text);
  return (
    <div className={className} data-quote-source={quoteSource}>
      {parts.map((part, index) => (
        <Fragment key={index}>
          {part.kind === "image" ? (
            <>
              <AgentMarkdownImage label={part.label} url={part.url} />
              {streaming && index === parts.length - 1 ? <StreamingCaret /> : null}
            </>
          ) : (
            <MarkdownRenderer streaming={streaming && index === parts.length - 1} text={part.text} />
          )}
        </Fragment>
      ))}
    </div>
  );
});

function MarkdownRenderer({ streaming, text }: { streaming: boolean; text: string }) {
  if (!text) return streaming ? <StreamingCaret /> : null;
  return (
    <MarkdownSourceContext.Provider value={text}>
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

const MarkdownSourceContext = createContext("");

// Stable component identities keep native pointer gestures alive across streamed Markdown updates.
const agentMarkdownComponents: Components = {
  a: ({ children, href, node: _node, ...props }) => {
    const label = plainText(children) || "Image";
    if (isSafeDataImageUrl(href)) {
      return <AgentMarkdownImage label={label} url={href} />;
    }
    const fileLocation = markdownFileLocation(href);
    if (fileLocation) {
      return (
        <a
          {...props}
          href={href}
          onClick={(event) => {
            event.preventDefault();
            postHostMessage({ type: "tool.openPath", payload: fileLocation });
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
  },
  pre: ({ children, node: _node, ...props }) => (
    <MarkdownCopyBlock kind="code" text={plainText(children).replace(/\n$/, "")}>
      <pre {...props}>{children}</pre>
    </MarkdownCopyBlock>
  ),
  blockquote: MarkdownQuoteBlock,
};

function MarkdownQuoteBlock({ children, node, ...props }: ComponentProps<"blockquote"> & ExtraProps) {
  const source = useContext(MarkdownSourceContext);
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
  return isSafeDataImageUrl(value) || markdownFileLocation(value) ? value : defaultUrlTransform(value);
}

// Agent file citations use an absolute path with an optional one-based line suffix.
function markdownFileLocation(href: string | undefined): { path: string; line?: number } | undefined {
  if (!href) return undefined;
  let decoded: string;
  try {
    decoded = decodeURIComponent(href);
  } catch {
    return undefined;
  }
  const lineSuffix = decoded.match(/^(.*):([1-9]\d*)$/);
  if (lineSuffix?.[1] && isAbsoluteFilePath(lineSuffix[1])) {
    return { path: lineSuffix[1], line: Number(lineSuffix[2]) };
  }
  return isAbsoluteFilePath(decoded) ? { path: decoded } : undefined;
}

function isAbsoluteFilePath(value: string) {
  return value.startsWith("/") || /^[a-z]:[\\/]/i.test(value);
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
