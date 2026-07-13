import { Brain, ChevronRight, Terminal, Wrench } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ActivityStep, ActivityToolDetails, NormalizedMessage } from "@openaide/app-shell-contracts";
import { toolDetailCacheKey } from "../state/store";
import { AgentMarkdown } from "./AgentMarkdown";
import { MessageCopyAction } from "./chatMessageActions";
import {
  activityStatusLabel,
  activityStepContext,
  activityStepLabel,
  activityStepPreview,
  activityStepStatus,
  activitySummary,
} from "../state/activityLabels";
import { hasToolDetails, toolKindClass } from "../state/toolDetailsViewModel";
import { toolPresentationName } from "../state/toolDetailsShared";
import { ChatToolDetails } from "./ChatToolDetailsView";
import { ToolCodeBlock } from "./ChatToolBlocks";
import { toolKindIcon } from "./chatToolIcons";

export function ChatActivityView({
  activity,
  onSubscribeToolDetail,
  taskId,
  toolDetails,
}: {
  activity: Extract<NormalizedMessage, { kind: "activity" }>;
  onSubscribeToolDetail?: (artifactId: string) => () => void;
  taskId: string;
  toolDetails?: Record<string, { loading: boolean; details?: ActivityToolDetails; error?: string }>;
}) {
  return (
    <AnimatedDisclosure
      className={`activity-group ${activity.status}`}
      trigger={
        <>
          <ChevronRight className="activity-disclosure-icon" size={13} aria-hidden="true" />
          <span className="activity-status-mark" aria-hidden="true" />
          <span>{activitySummary(activity)}</span>
          <small>{activityStatusLabel(activity.status)}</small>
        </>
      }
    >
      <div className="activity-step-list">
        {activity.steps.map((step, index) => (
          <ActivityStepRow
            key={activityStepIdentity(step) ?? index}
            onSubscribeToolDetail={onSubscribeToolDetail}
            step={step}
            taskId={taskId}
            toolDetails={toolDetails}
          />
        ))}
      </div>
    </AnimatedDisclosure>
  );
}

export function ActivityStepRow({
  onSubscribeToolDetail,
  step,
  taskId,
  toolDetails,
}: {
  onSubscribeToolDetail?: (artifactId: string) => () => void;
  step: ActivityStep;
  taskId: string;
  toolDetails?: Record<string, { loading: boolean; details?: ActivityToolDetails; error?: string }>;
}) {
  const artifactState =
    step.kind === "tool" && step.detail_artifact_id
      ? toolDetails?.[toolDetailCacheKey(taskId, step.detail_artifact_id)]
      : undefined;
  const details = step.kind === "tool" ? (artifactState?.details ?? step.details) : undefined;
  const displayStep: ActivityStep =
    step.kind === "tool" ? presentToolStep(step, details) : step;
  const label = activityStepLabel(displayStep);
  const preview = activityStepPreview(displayStep);
  const status = activityStepStatus(displayStep);
  const context = activityStepContext(displayStep);
  const metadata = <ActivityStepMetadata context={context === label ? undefined : context} status={status} />;
  const className = `activity-step ${displayStep.kind === "tool" ? `tool-${toolKindClass(displayStep.name)} ${displayStep.status}` : ""}`;
  const legacyCommandText = commandTextForExpandableLegacyStep(displayStep);
  if (step.kind === "thought") {
    return (
      <AnimatedDisclosure
        className="activity-step activity-thought-block"
        stepId={step.message_id}
        trigger={<ActivityStepContent disclosure icon={activityStepIcon(step)} label="Thought" />}
      >
        <AgentMarkdown className="chat-thought" text={step.text} />
        <MessageCopyAction text={step.text} />
      </AnimatedDisclosure>
    );
  }
  if (displayStep.kind === "command" || legacyCommandText) {
    const commandText = displayStep.kind === "command" ? displayStep.command_label : (legacyCommandText ?? label);
    const outputPreview = displayStep.kind === "command" || displayStep.kind === "tool" ? displayStep.output_preview : undefined;
    return (
      <AnimatedDisclosure
        className={commandStepClassName(displayStep, className)}
        stepId={displayStep.kind === "tool" ? displayStep.tool_call_id : undefined}
        trigger={
          <>
            <ActivityStepContent
              disclosure
              icon={activityStepIcon(displayStep)}
              label={
                <CommandStepTitle
                  command={commandText}
                  status={displayStep.kind === "command" || displayStep.kind === "tool" ? displayStep.status : "completed"}
                />
              }
              titleClassName="command"
            />
            {metadata}
          </>
        }
      >
        <div className="activity-tool-details">
          <ToolCodeBlock text={commandText} />
          {outputPreview ? <ToolCodeBlock text={outputPreview} /> : null}
        </div>
      </AnimatedDisclosure>
    );
  }
  if (displayStep.kind === "tool" && hasToolDetails(displayStep)) {
    const artifactId = displayStep.detail_artifact_id;
    return (
      <LiveToolDetailDisclosure
        artifactId={artifactId}
        artifactState={artifactState}
        className={className}
        details={details}
        metadata={metadata}
        onSubscribeToolDetail={onSubscribeToolDetail}
        preview={preview}
        step={displayStep}
      />
    );
  }
  return (
    <div className={className} data-step-id={displayStep.kind === "tool" ? displayStep.tool_call_id : undefined}>
      <ActivityStepContent icon={activityStepIcon(displayStep)} label={label} />
      {metadata}
      {preview ? <pre>{preview}</pre> : null}
    </div>
  );
}

function LiveToolDetailDisclosure({
  artifactId,
  artifactState,
  className,
  details,
  metadata,
  onSubscribeToolDetail,
  preview,
  step,
}: {
  artifactId?: string;
  artifactState?: { loading: boolean; details?: ActivityToolDetails; error?: string };
  className: string;
  details?: ActivityToolDetails;
  metadata: ReactNode;
  onSubscribeToolDetail?: (artifactId: string) => () => void;
  preview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const [open, setOpen] = useState(false);
  const subscribeToolDetailRef = useRef(onSubscribeToolDetail);
  subscribeToolDetailRef.current = onSubscribeToolDetail;
  useEffect(() => {
    if (!open || !artifactId) return undefined;
    return subscribeToolDetailRef.current?.(artifactId);
  }, [artifactId, open]);
  const commandTitle = step.name === "execute"
    ? <CommandStepTitle command={activityStepLabel(step)} status={step.status} />
    : activityStepLabel(step);
  return (
    <AnimatedDisclosure
      className={className}
      onOpenChange={setOpen}
      stepId={step.tool_call_id}
      trigger={(
        <>
          <ActivityStepContent
            disclosure
            icon={activityStepIcon(step)}
            label={commandTitle}
            titleClassName={step.name === "execute" ? "command" : undefined}
          />
          {metadata}
        </>
      )}
    >
      <ChatToolDetails
        details={details}
        error={artifactState?.error}
        fallbackPreview={preview}
        loading={artifactState?.loading}
        step={step}
      />
    </AnimatedDisclosure>
  );
}

function presentToolStep(
  step: Extract<ActivityStep, { kind: "tool" }>,
  details: ActivityToolDetails | undefined,
): Extract<ActivityStep, { kind: "tool" }> {
  const name = toolPresentationName(step.name, details);
  const inputSummary = name === "web_search" && step.name !== "web_search" ? details?.input?.query : step.input_summary;
  return { ...step, name, input_summary: inputSummary, ...(details ? { details } : {}) };
}

function ActivityStepContent({
  disclosure = false,
  icon,
  label,
  titleClassName,
}: {
  disclosure?: boolean;
  icon: ReactNode;
  label: ReactNode;
  titleClassName?: string;
}) {
  return (
    <span className="activity-step-main">
      {disclosure ? (
        <ChevronRight className="activity-step-disclosure" size={12} aria-hidden="true" />
      ) : (
        <span className="activity-step-disclosure-placeholder" aria-hidden="true" />
      )}
      {icon}
      <span className={["activity-step-title", titleClassName].filter(Boolean).join(" ")}>{label}</span>
    </span>
  );
}

function CommandStepTitle({ command, status }: { command: string; status: "running" | "completed" | "error" }) {
  return (
    <>
      <span className="activity-step-action">{status === "running" ? "Running" : "Ran"}</span>
      <code className="activity-step-command">{command}</code>
    </>
  );
}

function ActivityStepMetadata({ context, status }: { context?: string; status?: string }) {
  if (!context && !status) return null;
  return (
    <span className="activity-step-meta">
      {context ? <small className="activity-step-context">{context}</small> : null}
      {status ? <small className="activity-step-state">{status}</small> : null}
    </span>
  );
}

function AnimatedDisclosure({
  children,
  className,
  defaultOpen = false,
  onOpenChange,
  stepId,
  trigger,
}: {
  children: ReactNode;
  className: string;
  defaultOpen?: boolean;
  onOpenChange?: (open: boolean) => void;
  stepId?: string;
  trigger: ReactNode;
}) {
  const [open, setOpen] = useState(defaultOpen);
  const rootClassName = [className, open ? "open" : ""].filter(Boolean).join(" ");
  return (
    <div className={rootClassName} data-step-id={stepId}>
      <button
        aria-expanded={open}
        className="activity-disclosure-trigger"
        onClick={() => {
          const nextOpen = !open;
          setOpen(nextOpen);
          onOpenChange?.(nextOpen);
        }}
        type="button"
      >
        {trigger}
      </button>
      <div aria-hidden={!open} className={`activity-disclosure-body ${open ? "open" : ""}`} inert={!open}>
        <div className="activity-disclosure-content">{children}</div>
      </div>
    </div>
  );
}

export function activityStepIcon(step: ActivityStep) {
  if (step.kind === "thought") return <Brain className="activity-kind-icon" size={12} />;
  if (step.kind === "command" || (step.kind === "tool" && step.name === "execute")) {
    return <Terminal className="activity-kind-icon" size={12} />;
  }
  if (step.kind === "tool") return toolKindIcon(step.name, 12, "activity-kind-icon");
  return <Wrench className="activity-kind-icon" size={12} />;
}

function activityStepIdentity(step: ActivityStep) {
  if (step.kind === "thought") return step.message_id;
  if (step.kind === "tool") return step.tool_call_id;
  return undefined;
}

function commandStepClassName(step: ActivityStep, className: string) {
  if (step.kind === "tool") return className;
  return `activity-step tool-execute ${step.kind === "command" ? step.status : "completed"}`;
}

function commandTextForExpandableLegacyStep(step: ActivityStep) {
  if (step.kind === "text") return isCommandLikeText(step.text) ? step.text : undefined;
  if (step.kind !== "tool" || hasToolDetails(step)) return undefined;
  if (step.name === "execute" && step.input_summary) return step.input_summary;
  if (step.input_summary && isCommandLikeText(step.input_summary)) return step.input_summary;
  return undefined;
}

function isCommandLikeText(value: string) {
  const text = value.trim().toLowerCase();
  if (!text) return false;
  if (/(^|\s)(?:\/usr\/bin\/)?(?:bash|zsh|sh)\s+-lc\b/.test(text)) return true;
  return /^(?:git|npm|pnpm|yarn|cargo|go|node|python3?|pytest|npx|rg|grep|sed|cat|ls|curl|docker|deno|bun)\b/.test(text);
}
