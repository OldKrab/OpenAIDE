import { ChevronRight } from "lucide-react";
import { memo, useState } from "react";
import type { ActivityToolDetails, AgentCommandsCatalog, Attachment, ChatMessage, ElicitationResponse } from "@openaide/app-shell-contracts";
import { AgentMarkdown, splitDataImageMarkdown } from "./AgentMarkdown";
import { AttachmentImagePreviewLightbox, chatImagePreview, type AttachmentImagePreviewSource } from "./AttachmentImagePreview";
import { ChatActivityView } from "./ChatActivityView";
import { MessageCopyAction } from "./chatMessageActions";
import { ChatPermissionCard } from "./ChatPermissionCard";
import { QuestionCard } from "./QuestionCard";
import { SlashCommandText } from "./SlashCommandText";
import { UserMessageAttachments } from "./UserMessageAttachments";

export { firstToolPath } from "../state/toolDetailsViewModel";

export const ChatRow = memo(function ChatRow({
  message,
  onLoadToolDetail,
  onPermissionRespond,
  onQuestionRespond,
  permissionResponse,
  questionResponse,
  taskId,
  toolDetails,
  commandCatalog,
  showStreamingCaret = true,
}: {
  commandCatalog?: AgentCommandsCatalog;
  message: ChatMessage;
  onLoadToolDetail?: (artifactId: string, refresh?: boolean) => void;
  onPermissionRespond: (
    requestId: string,
    optionId: string,
    decision: "approved" | "denied",
    source?: "agent" | "appServer",
  ) => void;
  onQuestionRespond?: (requestId: string, response: ElicitationResponse) => void;
  permissionResponse?: { responding: boolean; error?: string };
  questionResponse?: { responding: boolean; error?: string };
  taskId: string;
  toolDetails?: Record<string, { loading: boolean; details?: ActivityToolDetails; error?: string }>;
  showStreamingCaret?: boolean;
}) {
  const [openImage, setOpenImage] = useState<AttachmentImagePreviewSource | undefined>();
  const body = message.message;
  if (body.kind === "user") {
    const hasText = body.text.trim().length > 0;
    return (
      <div className="chat-user-block">
        {body.attachments?.length ? <UserAttachments attachments={body.attachments} onOpenImage={setOpenImage} /> : null}
        {hasText ? <UserMessageText commandCatalog={commandCatalog} onOpenImage={setOpenImage} text={body.text} /> : null}
        {hasText ? <MessageCopyAction align="end" text={body.text} /> : null}
        {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={() => setOpenImage(undefined)} /> : null}
      </div>
    );
  }
  if (body.kind === "agent_text") {
    return <AgentTextMessage streaming={body.streaming === true && showStreamingCaret} text={body.text} />;
  }
  if (body.kind === "thought") {
    return (
      <details className="chat-thought-block">
        <summary>
          <ChevronRight className="chat-thought-disclosure" size={13} aria-hidden="true" />
          <span>Thinking</span>
        </summary>
        <AgentMarkdown className="chat-thought" text={body.text} />
        <MessageCopyAction text={body.text} />
      </details>
    );
  }
  if (body.kind === "activity") {
    return (
      <ChatActivityView activity={body} onLoadToolDetail={onLoadToolDetail} taskId={taskId} toolDetails={toolDetails} />
    );
  }
  if (body.kind === "interruption") {
    if (body.recoverable) {
      return (
        <section className="recovery-banner" role="status">
          <span>{body.message}</span>
        </section>
      );
    }
    return <p className="chat-system">{body.message}</p>;
  }
  if (body.kind === "permission") {
    return <ChatPermissionCard permission={body} response={permissionResponse} onRespond={onPermissionRespond} />;
  }
  if (body.kind === "elicitation") {
    return <QuestionCard elicitation={body} response={questionResponse} onRespond={onQuestionRespond ?? unavailableQuestionResponse} />;
  }
  return null;
});

function unavailableQuestionResponse() {}

function UserAttachments({
  attachments,
  onOpenImage,
}: {
  attachments: Attachment[];
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
}) {
  return (
    <UserMessageAttachments
      attachments={attachments.map((attachment, index) => ({
        id: attachment.id ?? `${attachment.label}-${index}`,
        image: chatImagePreview(attachment),
        label: attachment.label,
      }))}
      onOpenImage={onOpenImage}
    />
  );
}

function AgentTextMessage({ streaming, text }: { streaming: boolean; text: string }) {
  return (
    <div className="chat-agent-block" aria-busy={streaming || undefined}>
      <AgentMarkdown className="chat-agent" streaming={streaming} text={text} />
      <MessageCopyAction text={text} />
    </div>
  );
}

function UserMessageText({
  commandCatalog,
  onOpenImage,
  text,
}: {
  commandCatalog?: AgentCommandsCatalog;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
  text: string;
}) {
  const parts = splitDataImageMarkdown(text);
  if (parts.length === 1 && parts[0]?.kind === "markdown") {
    return <p className="chat-user"><SlashCommandText commands={commandCatalog} text={text} /></p>;
  }
  return (
    <div className="chat-user chat-user-rich-text">
      {parts.map((part, index) => (
        part.kind === "image" ? (
          <button
            aria-label={`Open ${part.label}`}
            className="chat-user-image-link"
            key={index}
            onClick={() => onOpenImage({ label: part.label, url: part.url })}
            type="button"
          >
            <img alt={part.label} src={part.url} />
          </button>
        ) : part.text.trim() ? (
          <p key={index}><SlashCommandText commands={commandCatalog} text={part.text} /></p>
        ) : null
      ))}
    </div>
  );
}
