import { Bot, Brain, ChevronRight, CircleX, Check, Terminal, Wrench } from "lucide-react";
import { Fragment, useEffect, useRef, useState, type ReactNode } from "react";
import type { ActivityStep, ActivityToolDetails, NormalizedMessage } from "@openaide/app-shell-contracts";
import type { ToolImagePreview } from "@openaide/app-server-client";
import { toolDetailCacheKey } from "../state/store";
import { AgentMarkdown } from "./AgentMarkdown";
import { MessageCopyAction } from "./chatMessageActions";
import {
  activityStatusLabel,
  activityStepContext,
  activityStepLabel,
  activityStepPreview,
  activityStepSemanticTitle,
  activityStepStatus,
  activitySummary,
  activityToolKind,
  type ActivityStepSemanticTitle,
} from "../state/activityLabels";
import { hasToolDetails, toolKindClass } from "../state/toolDetailsViewModel";
import { readDetailOutput, toolPresentationName } from "../state/toolDetailsShared";
import { skillDocumentName } from "../state/skillToolViewModel";
import { ChatToolDetails } from "./ChatToolDetailsView";
import { ToolCodeBlock } from "./ChatToolBlocks";
import { toolKindIcon } from "./chatToolIcons";
import { presentThoughtMarkdown } from "./thoughtPresentation";

export function ChatActivityView({
  activity,
  onLoadToolImagePreview,
  onSubscribeToolDetail,
  taskId,
  toolDetails,
}: {
  activity: Extract<NormalizedMessage, { kind: "activity" }>;
  onLoadToolImagePreview?: (artifactId: string) => Promise<ToolImagePreview | undefined>;
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
              legacyToolName={activity.steps.length === 1 ? activity.title : undefined}
              onLoadToolImagePreview={onLoadToolImagePreview}
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
  legacyToolName,
  onLoadToolImagePreview,
  onSubscribeToolDetail,
  step,
  taskId,
  toolDetails,
}: {
  legacyToolName?: string;
  onLoadToolImagePreview?: (artifactId: string) => Promise<ToolImagePreview | undefined>;
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
  const semanticTitle = activityStepSemanticTitle(displayStep);
  const title = semanticTitle ? <SemanticStepTitle title={semanticTitle} /> : label;
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
  const className = `activity-step ${displayStep.kind === "tool"
    ? [
        `tool-${toolKindClass(displayStep.name)}`,
        displayStep.presentation ? `tool-presentation-${toolKindClass(activityToolKind(displayStep))}` : "",
        displayStep.status,
      ].filter(Boolean).join(" ")
    : displayStep.kind === "subagent"
      ? `activity-subagent ${displayStep.status}`
      : ""}`;
  const legacyCommandText = commandTextForExpandableLegacyStep(displayStep);
  if (step.kind === "thought") {
    return (
      <AnimatedDisclosure
        className="activity-step activity-thought-block"
        stepId={step.message_id}
        trigger={<ActivityStepContent disclosure icon={activityStepIcon(step, legacyToolName)} label="Thought" />}
      >
        <AgentMarkdown className="chat-thought" text={presentThoughtMarkdown(step.text)} />
        <MessageCopyAction text={step.text} />
      </AnimatedDisclosure>
    );
  }
  if (displayStep.kind === "subagent") {
    const hasProtocolDetails = Boolean(
      displayStep.title && displayStep.thread_id && displayStep.raw_path && displayStep.activity,
    );
    return (
      <AnimatedDisclosure
        className={className}
        stepId={displayStep.tool_call_id}
        trigger={(
          <>
            <ActivityStepContent
              disclosure
              icon={activityStepIcon(displayStep, legacyToolName)}
              label={title}
              tooltip={semanticTitle?.tooltip ?? label}
            />
            {metadata}
          </>
        )}
      >
        {hasProtocolDetails ? (
          <dl className="subagent-tool-details">
            <div>
              <dt>Path</dt>
              <dd><code>{displayStep.raw_path}</code></dd>
            </div>
            <div>
              <dt>Thread ID</dt>
              <dd><code>{displayStep.thread_id}</code></dd>
            </div>
            <div>
              <dt>Activity</dt>
              <dd>{displayStep.activity}</dd>
            </div>
          </dl>
        ) : (
          <ol className="subagent-activity-history">
            {displayStep.events.map((event, index) => (
              <li key={`${event}:${index}`}>{subagentEventLabel(event)}</li>
            ))}
          </ol>
        )}
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
              icon={activityStepIcon(displayStep, legacyToolName)}
              label={
                <CommandStepTitle
                  command={commandText}
                  status={displayStep.kind === "command" || displayStep.kind === "tool" ? displayStep.status : "completed"}
                />
              }
              titleClassName="command"
              tooltip={commandText}
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
        legacyToolName={legacyToolName}
        metadata={metadata}
        onLoadToolImagePreview={onLoadToolImagePreview}
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
            <ActivityStepContent
              disclosure
              icon={activityStepIcon(displayStep, legacyToolName)}
              label={title}
              titleClassName={semanticTitle ? "semantic" : undefined}
              tooltip={label}
            />
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
      <ActivityStepContent
        icon={activityStepIcon(displayStep, legacyToolName)}
        label={title}
        titleClassName={semanticTitle ? "semantic" : undefined}
        tooltip={semanticTitle?.tooltip ?? label}
      />
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
  legacyToolName,
  metadata,
  onLoadToolImagePreview,
  onSubscribeToolDetail,
  preview,
  step,
}: {
  artifactId?: string;
  artifactState?: { loading: boolean; details?: ActivityToolDetails; error?: string };
  className: string;
  details?: ActivityToolDetails;
  legacyToolName?: string;
  metadata: ReactNode;
  onLoadToolImagePreview?: (artifactId: string) => Promise<ToolImagePreview | undefined>;
  onSubscribeToolDetail?: (artifactId: string) => () => void;
  preview?: string;
  step: Extract<ActivityStep, { kind: "tool" }>;
}) {
  const [open, setOpen] = useState(false);
  const [imagePreview, setImagePreview] = useState<ToolImagePreview>();
  const loadToolImagePreviewRef = useRef(onLoadToolImagePreview);
  loadToolImagePreviewRef.current = onLoadToolImagePreview;
  const subscribeToolDetailRef = useRef(onSubscribeToolDetail);
  subscribeToolDetailRef.current = onSubscribeToolDetail;
  useEffect(() => {
    if (!open || !artifactId) return undefined;
    return subscribeToolDetailRef.current?.(artifactId);
  }, [artifactId, open]);
  const detailsAvailable = details !== undefined;
  useEffect(() => {
    const loadImagePreview = loadToolImagePreviewRef.current;
    if (!open || !artifactId || !detailsAvailable || !loadImagePreview) {
      setImagePreview(undefined);
      return undefined;
    }
    let active = true;
    setImagePreview(undefined);
    void loadImagePreview(artifactId)
      .then((preview) => {
        if (active) setImagePreview(preview);
      })
      .catch(() => {
        if (active) setImagePreview(undefined);
      });
    return () => {
      active = false;
    };
  }, [artifactId, detailsAvailable, open]);
  const semanticTitle = activityStepSemanticTitle(step);
  const commandTitle = semanticTitle
    ? <SemanticStepTitle title={semanticTitle} />
    : step.name === "execute" && !step.presentation
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
            icon={activityStepIcon(step, legacyToolName)}
            label={commandTitle}
            titleClassName={semanticTitle ? "semantic" : step.name === "execute" && !step.presentation ? "command" : undefined}
            tooltip={semanticTitle?.tooltip ?? activityStepLabel(step)}
          />
          {metadata}
        </>
      )}
    >
      <ChatToolDetails
        details={details}
        error={artifactState?.error}
        fallbackPreview={preview}
        imagePreview={imagePreview}
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
  const name = toolPresentationName(step.name, details, step.output_preview);
  const output = details ? readDetailOutput(details, step.output_preview) : step.output_preview ?? "";
  const inputSummary = name === "skill"
    ? skillDocumentName(output)
    : name === "web_search" && step.name !== "web_search"
      ? details?.input?.query
      : step.input_summary;
  return { ...step, name, input_summary: inputSummary, ...(details ? { details } : {}) };
}

function ActivityStepContent({
  disclosure = false,
  icon,
  label,
  preview,
  titleClassName,
  tooltip,
}: {
  disclosure?: boolean;
  icon: ReactNode;
  label: ReactNode;
  preview?: string;
  titleClassName?: string;
  tooltip?: string;
}) {
  return (
    <span className="activity-step-main">
      {disclosure ? (
        <ChevronRight className="activity-step-disclosure" size={12} aria-hidden="true" />
      ) : (
        <span className="activity-step-disclosure-placeholder" aria-hidden="true" />
      )}
      {icon}
      <span
        className={["activity-step-title", titleClassName].filter(Boolean).join(" ")}
        title={tooltip ?? (typeof label === "string" ? label : undefined)}
      >
        {label}
      </span>
      {preview ? <span className="activity-step-preview" title={preview}>{preview}</span> : null}
    </span>
  );
}

function CommandStepTitle({ command, status }: { command: string; status: "running" | "completed" | "error" | "interrupted" }) {
  const action = status === "running" ? "Running" : status === "interrupted" ? "Interrupted" : "Ran";
  return (
    <>
      <span className="activity-step-action">{action}</span>
      <code className="activity-step-command">{command}</code>
    </>
  );
}

function semanticSubjectListClassName(
  action: ActivityStepSemanticTitle["actions"][number],
) {
  const tone = action.action === "Activated"
    ? "identity"
    : action.action === "List" || action.action === "Read" || action.action === "View"
      ? "resource"
      : "technical";
  // Preserve short patterns at natural width; only long Search subjects need a local text window.
  const bounded = action.action.startsWith("Search")
    && action.subjects.some((subject) => Array.from(subject).length > 32);
  return ["activity-step-semantic-subject-list", tone, bounded ? "bounded" : ""]
    .filter(Boolean)
    .join(" ");
}

function SemanticStepTitle({ title }: { title: ActivityStepSemanticTitle }) {
  return (
    <>
      {title.actions.map((action, actionIndex) => (
        <Fragment key={`${action.action}-${actionIndex}`}>
          {actionIndex > 0 ? (
            <span className="activity-step-semantic-action-separator" aria-label="then">
              →
            </span>
          ) : null}
          <span className="activity-step-semantic-action">{action.action}</span>
          <span
            className={semanticSubjectListClassName(action)}
          >
            {action.subjects.map((subject, subjectIndex) => (
              <Fragment key={`${subject}-${subjectIndex}`}>
              {subjectIndex > 0 ? (
                <span className="activity-step-semantic-connector">
                  {subjectIndex === action.subjects.length - 1 ? " and " : ", "}
                </span>
              ) : null}
                <span className="activity-step-semantic-subject">{subject}</span>
              </Fragment>
            ))}
          </span>
          {action.scope ? (
            <>
              <span className="activity-step-semantic-connector">in</span>
              <span className="activity-step-semantic-scope">{action.scope}</span>
            </>
          ) : null}
          </Fragment>
      ))}
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

export function activityStepIcon(step: ActivityStep, legacyToolName?: string) {
  if (step.kind === "thought") return <Brain className="activity-kind-icon" size={12} />;
  if (step.kind === "subagent") return <Bot className="activity-kind-icon" size={12} />;
  if (step.kind === "command" || (step.kind === "tool" && step.name === "execute" && !step.presentation)) {
    return <Terminal className="activity-kind-icon" size={12} />;
  }
  if (step.kind === "tool") return toolKindIcon(activityToolKind(step, legacyToolName), 12, "activity-kind-icon");
  return <Wrench className="activity-kind-icon" size={12} />;
}

function activityStepIdentity(step: ActivityStep) {
  if (step.kind === "thought") return step.message_id;
  if (step.kind === "tool") return step.tool_call_id;
  if (step.kind === "subagent") return step.tool_call_id;
  return undefined;
}

function subagentEventLabel(event: Extract<ActivityStep, { kind: "subagent" }>["events"][number]) {
  const labels = {
    delegated: "Delegated work",
    interacted: "Checked in with subagent",
    running: "Subagent reported running",
    completed: "Subagent completed",
    failed: "Subagent failed",
    stopped: "Subagent stopped",
  } satisfies Record<Extract<ActivityStep, { kind: "subagent" }>["events"][number], string>;
  return labels[event];
}

function commandStepClassName(step: ActivityStep, className: string) {
  if (step.kind === "tool") return className;
  return `activity-step tool-execute ${step.kind === "command" ? step.status : "completed"}`;
}

function commandTextForExpandableLegacyStep(step: ActivityStep) {
  if (step.kind === "text") return isCommandLikeText(step.text) ? step.text : undefined;
  if (step.kind !== "tool" || hasToolDetails(step)) return undefined;
  if (step.presentation) return undefined;
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
