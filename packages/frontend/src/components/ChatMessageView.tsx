import { CircleAlert, ChevronRight, FileText } from "lucide-react";
import { memo, useRef, useState } from "react";
import type { ActivityToolDetails, AgentCommandsCatalog, AgentMessagePart, Attachment, ChatMessage, ElicitationResponse } from "@openaide/app-shell-contracts";
import { AgentMarkdown, splitDataImageMarkdown } from "./AgentMarkdown";
import { AttachmentImagePreviewLightbox, chatImagePreview, type AttachmentImagePreviewSource } from "./AttachmentImagePreview";
import { ChatActivityView } from "./ChatActivityView";
import { MessageCopyAction } from "./chatMessageActions";
import { ChatPermissionCard } from "./ChatPermissionCard";
import { ReferenceHoverLayer } from "./ComposerReferenceHover";
import { QuestionCard } from "./QuestionCard";
import { SlashCommandText } from "./SlashCommandText";
import { UserMessageAttachments } from "./UserMessageAttachments";
import { useLiveMessagePresentation } from "./useLiveMessagePresentation";

export { firstToolPath } from "../state/toolDetailsViewModel";

export const ChatRow = memo(function ChatRow({
  message,
  onSubscribeToolDetail,
  onPermissionRespond,
  onQuestionRespond,
  permissionResponse,
  questionResponse,
  taskId,
  toolDetails,
  commandCatalog,
  showStreamingCaret = false,
  liveTextEventCursor,
  presentLiveText = false,
}: {
  commandCatalog?: AgentCommandsCatalog;
  message: ChatMessage;
  onSubscribeToolDetail?: (artifactId: string) => () => void;
  onPermissionRespond: (
    requestId: string,
    optionId: string,
  ) => void;
  onQuestionRespond?: (requestId: string, response: ElicitationResponse) => void;
  permissionResponse?: { responding: boolean; error?: string };
  questionResponse?: { responding: boolean; error?: string };
  taskId: string;
  toolDetails?: Record<string, { loading: boolean; details?: ActivityToolDetails; error?: string }>;
  showStreamingCaret?: boolean;
  liveTextEventCursor?: string;
  presentLiveText?: boolean;
}) {
  const [openImage, setOpenImage] = useState<AttachmentImagePreviewSource | undefined>();
  const referenceRootRef = useRef<HTMLDivElement | null>(null);
  const body = message.message;
  if (body.kind === "user") {
    const hasText = body.text.trim().length > 0;
    return (
      <div className="chat-user-block" ref={referenceRootRef}>
        {body.attachments?.length ? <UserAttachments attachments={body.attachments} onOpenImage={setOpenImage} /> : null}
        {hasText ? <UserMessageText commandCatalog={commandCatalog} onOpenImage={setOpenImage} text={body.text} /> : null}
        {hasText ? <MessageCopyAction align="end" text={body.text} /> : null}
        <ReferenceHoverLayer contentKey={body.text} rootRef={referenceRootRef} />
        {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={() => setOpenImage(undefined)} /> : null}
      </div>
    );
  }
  if (body.kind === "agent_message") {
    return (
      <AgentMessageRow
        body={body}
        liveTextEventCursor={liveTextEventCursor}
        onCloseImage={() => setOpenImage(undefined)}
        onOpenImage={setOpenImage}
        openImage={openImage}
        presentLiveText={presentLiveText}
        showStreamingCaret={showStreamingCaret}
      />
    );
  }
  if (body.kind === "activity") {
    return (
      <ChatActivityView activity={body} onSubscribeToolDetail={onSubscribeToolDetail} taskId={taskId} toolDetails={toolDetails} />
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

function AgentMessageRow({
  body,
  liveTextEventCursor,
  onCloseImage,
  onOpenImage,
  openImage,
  presentLiveText,
  showStreamingCaret,
}: {
  body: Extract<ChatMessage["message"], { kind: "agent_message" }>;
  liveTextEventCursor?: string;
  onCloseImage: () => void;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
  openImage?: AttachmentImagePreviewSource;
  presentLiveText: boolean;
  showStreamingCaret: boolean;
}) {
  const presentation = useLiveMessagePresentation({
    enabled: presentLiveText,
    eventCursor: liveTextEventCursor,
    parts: body.parts,
  });
  const streaming = showStreamingCaret || presentation.streaming;
  const text = agentMessageText(presentation.parts);
  const content = (
    <AgentMessageParts
      muted={body.role === "thought"}
      onOpenImage={onOpenImage}
      parts={presentation.parts}
      streaming={streaming}
    />
  );
  if (body.role === "thought") {
    return (
      <>
        <details aria-busy={streaming || undefined} className="chat-thought-block">
          <summary>
            <ChevronRight className="chat-thought-disclosure" size={13} aria-hidden="true" />
            <span>Thinking</span>
          </summary>
          {content}
          {text ? <MessageCopyAction text={text} /> : null}
        </details>
        {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={onCloseImage} /> : null}
      </>
    );
  }
  return (
    <>
      <div className="chat-agent-block" aria-busy={streaming || undefined}>
        {content}
        {text ? <MessageCopyAction text={text} /> : null}
      </div>
      {openImage ? <AttachmentImagePreviewLightbox image={openImage} onClose={onCloseImage} /> : null}
    </>
  );
}

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

function AgentMessageParts({
  parts,
  streaming,
  ...contentProps
}: {
  parts: AgentMessagePart[];
  streaming: boolean;
  muted: boolean;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
}) {
  return parts.map((part, index) => part.kind === "text" ? (
    <AgentMarkdown
      className={contentProps.muted ? "chat-thought" : "chat-agent"}
      key={index}
      streaming={streaming && index === parts.length - 1}
      text={part.text}
    />
  ) : (
    <AgentContentMessage content={part} key={index} {...contentProps} />
  ));
}

function AgentContentMessage({
  content,
  muted,
  onOpenImage,
}: {
  content: Exclude<AgentMessagePart, { kind: "text" }>;
  muted: boolean;
  onOpenImage: (image: AttachmentImagePreviewSource) => void;
}) {
  if (content.kind === "image") {
    const label = content.uri ? resourceLabel(content.uri) : "Agent image";
    return (
      <button
        aria-label={`Open ${label}`}
        className="chat-agent-content-image"
        onClick={() => onOpenImage({ label, url: content.data_url })}
        type="button"
      >
        <img alt={label} src={content.data_url} />
      </button>
    );
  }
  if (content.kind === "resource") {
    const label = content.title || content.name || resourceLabel(content.uri);
    const metadata = [content.media_type, formatByteSize(content.size_bytes)].filter(Boolean).join(" · ");
    const header = (
      <>
        <FileText aria-hidden="true" size={14} />
        <span className="chat-agent-content-title">{label}</span>
        {metadata ? <span className="chat-agent-content-meta">{metadata}</span> : null}
      </>
    );
    if (content.text !== undefined) {
      return (
        <details className={`chat-agent-content-resource${muted ? " muted" : ""}`}>
          <summary>{header}</summary>
          {content.description ? <p>{content.description}</p> : null}
          <pre>{content.text}</pre>
        </details>
      );
    }
    return (
      <section className={`chat-agent-content-resource${muted ? " muted" : ""}`}>
        <div className="chat-agent-content-resource-heading">{header}</div>
        {content.description ? <p>{content.description}</p> : null}
        <code>{content.uri}</code>
      </section>
    );
  }
  const label = content.content_type === "audio" ? "Audio output" : "Binary resource";
  return (
    <section className={`chat-agent-content-unsupported${muted ? " muted" : ""}`}>
      <CircleAlert aria-hidden="true" size={14} />
      <span>{label} is not previewable yet.</span>
      {content.media_type ? <code>{content.media_type}</code> : null}
    </section>
  );
}

function resourceLabel(uri: string) {
  const withoutQuery = uri.split(/[?#]/, 1)[0] ?? uri;
  const segment = withoutQuery.split("/").filter(Boolean).at(-1);
  return segment || "Agent resource";
}

function formatByteSize(size: number | undefined) {
  if (size === undefined) return undefined;
  if (size < 1024) return `${size} B`;
  if (size < 1024 * 1024) return `${(size / 1024).toFixed(1)} KB`;
  return `${(size / (1024 * 1024)).toFixed(1)} MB`;
}

function agentMessageText(parts: AgentMessagePart[]) {
  return parts
    .filter((part): part is Extract<AgentMessagePart, { kind: "text" }> => part.kind === "text")
    .map((part) => part.text)
    .join("");
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
