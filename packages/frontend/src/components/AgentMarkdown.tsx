import { Fragment, type ReactNode } from "react";
import Markdown, { defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";

type AgentMarkdownProps = {
  className?: string;
  streaming?: boolean;
  text: string;
};

export function AgentMarkdown({ className, streaming = false, text }: AgentMarkdownProps) {
  const parts = splitDataImageMarkdown(text);
  return (
    <div className={className}>
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
}

function MarkdownRenderer({ streaming, text }: { streaming: boolean; text: string }) {
  if (!text) return streaming ? <StreamingCaret /> : null;
  return (
    <Markdown
      rehypePlugins={streaming ? [appendStreamingCaret] : []}
      remarkPlugins={[remarkGfm]}
      skipHtml
      urlTransform={safeMarkdownUrl}
      components={{
        a: ({ children, href, node: _node, ...props }) => {
          const label = plainText(children) || "Image";
          if (isSafeDataImageUrl(href)) {
            return <AgentMarkdownImage label={label} url={href} />;
          }
          return href ? (
            <a {...props} href={href} rel="noreferrer" target="_blank">
              {children}
            </a>
          ) : (
            <span>{children}</span>
          );
        },
      }}
    >
      {text}
    </Markdown>
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

// Append inside the final rendered block, but outside inline formatting so the caret does not
// inherit link, emphasis, or inline-code styling.
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
  return isSafeDataImageUrl(value) ? value : defaultUrlTransform(value);
}

function isSafeDataImageUrl(value: unknown): value is string {
  return typeof value === "string" && /^data:image\/[a-z0-9.+-]+;base64,[a-z0-9+/=\s]+$/i.test(value);
}

function plainText(children: ReactNode): string {
  if (typeof children === "string" || typeof children === "number") return String(children);
  if (Array.isArray(children)) return children.map(plainText).join("");
  return "";
}
