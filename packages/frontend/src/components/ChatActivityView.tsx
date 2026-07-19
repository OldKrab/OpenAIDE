import { Brain, ChevronRight, CircleX, Check, Terminal, Wrench } from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";
import type { ActivityStep, ActivityToolDetails, NormalizedMessage } from "@openaide/app-shell-contracts";
import { toolDetailCacheKey } from "../state/store";
import { AgentMarkdown } from "./AgentMarkdown";
import { MessageCopyAction } from "./chatMessageActions";
import {
  activityStatusLabel,
  activityPresentationStatus,
  activityCommandActionLabel,
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
  // Longer reasoning runs stay recoverable without overwhelming the default activity scan.
  const [showThoughts, setShowThoughts] = useState(false);
  const thoughtCount = activity.steps.filter((step) => step.kind === "thought").length;
  const hasNonThoughtStep = activity.steps.some((step) => step.kind !== "thought");
  // A Thought-only group is itself the reasoning disclosure, so opening it must
  // reveal the complete run without a second visibility control.
  const thoughtsAreCollapsible = hasNonThoughtStep && thoughtCount > 2;
  return (
    <AnimatedDisclosure
      className={`activity-group ${activityPresentationStatus(activity.status)}`}
      trigger={
        <>
          <ChevronRight className="activity-disclosure-icon" size={13} aria-hidden="true" />
          <span className="activity-status-mark" aria-hidden="true" />
          <span>{activitySummary(activity)}</span>
          {activity.status === "completed" ? null : <small>{activityStatusLabel(activity.status)}</small>}
        </>
      }
    >
      <div className="activity-step-list">
        {thoughtsAreCollapsible ? (
          <button
            aria-expanded={showThoughts}
            className="activity-reasoning-toggle"
            onClick={() => setShowThoughts((visible) => !visible)}
            type="button"
          >
            <Brain className="activity-reasoning-toggle-icon" size={13} aria-hidden="true" />
            <span>{showThoughts ? "Reasoning visible in chronological order" : thoughtCountLabel(thoughtCount)}</span>
            <span className="activity-reasoning-toggle-action">{showThoughts ? "Hide" : "Show"}</span>
          </button>
        ) : null}
        {activity.steps.map((step, index) => {
          if (step.kind === "thought" && thoughtsAreCollapsible && !showThoughts) return null;
          return (
            <ActivityStepRow
              key={activityStepIdentity(step) ?? index}
              onSubscribeToolDetail={onSubscribeToolDetail}
              step={step}
              taskId={taskId}
              toolDetails={toolDetails}
            />
          );
        })}
      </div>
    </AnimatedDisclosure>
  );
}

function thoughtCountLabel(count: number) {
  return `${count} ${count === 1 ? "Thought" : "Thoughts"} hidden`;
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
  const permissionSummary = displayStep.kind === "tool"
    ? toolPermissionSummary(displayStep.permission_outcomes ?? [])
    : undefined;
  const metadata = (
    <ActivityStepMetadata
      context={context === label ? undefined : context}
      permissionSummary={permissionSummary}
      status={status}
    />
  );
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
          {displayStep.kind === "tool" ? <ToolPermissionOutcomes outcomes={displayStep.permission_outcomes ?? []} /> : null}
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
  if (displayStep.kind === "tool" && displayStep.permission_outcomes?.length) {
    return (
      <AnimatedDisclosure
        className={className}
        stepId={displayStep.tool_call_id}
        trigger={(
          <>
            <ActivityStepContent disclosure icon={activityStepIcon(displayStep)} label={label} />
            {metadata}
          </>
        )}
      >
        <ToolPermissionOutcomes outcomes={displayStep.permission_outcomes} />
      </AnimatedDisclosure>
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
      <ToolPermissionOutcomes outcomes={step.permission_outcomes ?? []} />
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

function CommandStepTitle({ command, status }: { command: string; status: "running" | "completed" | "error" | "interrupted" }) {
  const action = activityCommandActionLabel(status);
  return (
    <>
      <span className="activity-step-action">{action}</span>
      <code className="activity-step-command">{command}</code>
    </>
  );
}

function ActivityStepMetadata({
  context,
  permissionSummary,
  status,
}: {
  context?: string;
  permissionSummary?: { decision: "approved" | "rejected" | "cancelled"; label: string };
  status?: string;
}) {
  if (!context && !permissionSummary && !status) return null;
  return (
    <span className="activity-step-meta">
      {context ? <small className="activity-step-context">{context}</small> : null}
      {permissionSummary ? (
        <small className={`activity-step-approval ${permissionSummary.decision}`}>
          {permissionSummary.decision === "approved" ? <Check size={12} aria-hidden="true" /> : <CircleX size={12} aria-hidden="true" />}
          {permissionSummary.label}
        </small>
      ) : null}
      {status ? <small className="activity-step-state">{status}</small> : null}
    </span>
  );
}

function ToolPermissionOutcomes({
  outcomes,
}: {
  outcomes: NonNullable<Extract<ActivityStep, { kind: "tool" }>["permission_outcomes"]>;
}) {
  if (!outcomes.length) return null;
  return (
    <section className="tool-permission-history" aria-label="Permission decisions">
      <span className="activity-tool-section-title">Permissions</span>
      <ul>
        {outcomes.map((outcome) => (
          <li className={outcome.decision} key={outcome.request_id}>
            {outcome.decision === "approved" ? <Check size={12} aria-hidden="true" /> : <CircleX size={12} aria-hidden="true" />}
            <span>{outcome.option_label ?? permissionDecisionLabel(outcome.decision)}</span>
          </li>
        ))}
      </ul>
    </section>
  );
}

function toolPermissionSummary(
  outcomes: NonNullable<Extract<ActivityStep, { kind: "tool" }>["permission_outcomes"]>,
) {
  const approved = outcomes.filter((outcome) => outcome.decision === "approved").length;
  const rejected = outcomes.filter((outcome) => outcome.decision === "rejected").length;
  const cancelled = outcomes.filter((outcome) => outcome.decision === "cancelled").length;
  const labels = [
    permissionCountLabel(approved, "Approved", "approvals"),
    permissionCountLabel(rejected, "Rejected", "rejections"),
    permissionCountLabel(cancelled, "Cancelled", "cancelled"),
  ].filter((label): label is string => Boolean(label));
  if (!labels.length) return undefined;
  const decision = rejected ? "rejected" as const : cancelled && !approved ? "cancelled" as const : "approved" as const;
  return { decision, label: labels.join(" · ") };
}

function permissionCountLabel(count: number, singular: string, plural: string) {
  if (!count) return undefined;
  return count === 1 ? singular : `${count} ${plural}`;
}

function permissionDecisionLabel(decision: "approved" | "rejected" | "cancelled") {
  if (decision === "approved") return "Approved";
  if (decision === "rejected") return "Rejected";
  return "Cancelled";
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
