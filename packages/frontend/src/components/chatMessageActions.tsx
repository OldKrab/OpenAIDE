import { Check, Copy } from "lucide-react";
import { useState, type ReactNode } from "react";
import { copyText } from "./clipboard";

/** The message's hover row: optional time information, then Copy when the message has text. */
export function MessageCopyAction({
  align = "start",
  leading,
  text,
}: {
  align?: "start" | "end";
  /** Time information shown before Copy; it shares the row's hover reveal. */
  leading?: ReactNode;
  text?: string;
}) {
  const [copied, setCopied] = useState(false);
  return (
    <div className={`chat-message-actions ${align}`}>
      {leading}
      {text ? <button
        aria-label={copied ? "Copied message" : "Copy message"}
        className="chat-message-action"
        onClick={async () => {
          await copyText(text);
          setCopied(true);
          window.setTimeout(() => setCopied(false), 1400);
        }}
        title={copied ? "Copied" : "Copy"}
        type="button"
      >
        {copied ? <Check size={13} /> : <Copy size={13} />}
      </button> : null}
    </div>
  );
}
