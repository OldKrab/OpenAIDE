import { Fragment, type ReactNode } from "react";
import Markdown, { defaultUrlTransform } from "react-markdown";
import remarkGfm from "remark-gfm";

type AgentMarkdownProps = {
  className?: string;
  text: string;
};

export function AgentMarkdown({ className, text }: AgentMarkdownProps) {
  const parts = splitDataImageMarkdown(text);
  return (
    <div className={className}>
      {parts.map((part, index) => (
        <Fragment key={index}>
          {part.kind === "image" ? (
            <AgentMarkdownImage label={part.label} url={part.url} />
          ) : (
            <MarkdownRenderer text={part.text} />
          )}
        </Fragment>
      ))}
    </div>
  );
}

function MarkdownRenderer({ text }: { text: string }) {
  if (!text) return null;
  return (
    <Markdown
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
