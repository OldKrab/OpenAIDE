import { Check, Copy } from "lucide-react";
import { useState } from "react";

export function MessageCopyAction({ align = "start", text }: { align?: "start" | "end"; text: string }) {
  const [copied, setCopied] = useState(false);
  return (
    <div className={`chat-message-actions ${align}`}>
      <button
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
      </button>
    </div>
  );
}

async function copyText(text: string) {
  if (navigator.clipboard?.writeText) {
    await navigator.clipboard.writeText(text);
    return;
  }
  const textArea = document.createElement("textarea");
  textArea.value = text;
  textArea.setAttribute("readonly", "");
  textArea.style.position = "fixed";
  textArea.style.opacity = "0";
  document.body.append(textArea);
  textArea.select();
  document.execCommand("copy");
  textArea.remove();
}
