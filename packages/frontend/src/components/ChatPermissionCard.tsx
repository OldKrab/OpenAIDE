import { Check, CheckCheck, CircleAlert, CircleX, Folder, LoaderCircle, ShieldCheck, ShieldX, X } from "lucide-react";
import type { ActivityStep, ActivityToolDetails, NormalizedMessage, PermissionOption, PermissionOptionKind } from "@openaide/app-shell-contracts";
import type { ToolImagePreview } from "@openaide/app-server-client";
import { toolKindClass } from "../state/toolDetailsViewModel";
import { ActivityStepRow } from "./ChatActivityView";
import { toolKindIcon } from "./chatToolIcons";

type ToolStep = Extract<ActivityStep, { kind: "tool" }>;

export function ChatPermissionCard({
  onLoadToolImagePreview,
  onRespond,
  onSubscribeToolDetail,
  permission,
  queued = 0,
  relatedTool,
  response,
  taskId,
  toolDetails,
}: {
  permission: Extract<NormalizedMessage, { kind: "permission" }>;
  onLoadToolImagePreview?: (artifactId: string) => Promise<ToolImagePreview | undefined>;
  queued?: number;
  relatedTool?: ToolStep;
  response?: { responding: boolean; error?: string };
  onRespond: (
    requestId: string,
    optionId: string,
  ) => void;
  onSubscribeToolDetail?: (artifactId: string) => () => void;
  taskId?: string;
  toolDetails?: Record<string, { loading: boolean; details?: ActivityToolDetails; error?: string }>;
}) {
  const selected = permission.options.find((option) => option.id === permission.selected_option);
  const terminal = permission.state === "resolved" || permission.state === "cancelled";
  const approved = permission.state === "resolved" && permission.decision === "approved";
  const responding = response?.responding ?? false;
  const resolution = terminal ? permissionResolutionLabel(permission, selected) : undefined;
  const display = permissionDisplay(permission);
  const showCommand = Boolean(display.chip && display.chip !== display.title);
  const showFacts = Boolean(permission.scope || permission.risk);
  const showBody = !terminal || Boolean(display.description || showCommand || showFacts || response?.error);

  const respond = (option: PermissionOption, action?: HTMLButtonElement) => {
    if (responding || terminal) return;
    const decision = permissionDecisionForOption(option);
    if (!decision) return;
    // The action row disappears after resolution. Keep focus on the stable card
    // without letting native focus restoration move the Chat viewport.
    action?.closest<HTMLElement>(".permission-card")?.focus({ preventScroll: true });
    onRespond(
      permission.app_server_request_id ?? permission.request_id,
      option.id,
    );
  };

  return (
    <section
      className={`permission-card tool-${toolKindClass(permission.tool_call.kind ?? "other")} ${terminal ? "resolved" : "pending"}`}
      aria-label="Permission request"
      tabIndex={-1}
    >
      <header className="permission-head">
        <span className="permission-icon" aria-hidden="true">
          {terminal ? toolKindIcon(permission.tool_call.kind, 14) : <ShieldCheck size={14} />}
        </span>
        <span
          aria-atomic={!terminal && !responding ? "true" : undefined}
          aria-live={!terminal && !responding ? "polite" : undefined}
          className="permission-title"
          role={!terminal && !responding ? "status" : undefined}
        >
          <strong>{terminal ? display.title : "Approval required"}</strong>
        </span>
        {responding || terminal ? (
          <span
            aria-atomic="true"
            aria-live="polite"
            className={`permission-state ${responding ? "responding" : approved ? "approved" : permission.state === "cancelled" ? "cancelled" : "denied"}`}
            role="status"
          >
            {responding
              ? <LoaderCircle size={14} aria-hidden="true" />
              : approved
                ? <Check size={14} aria-hidden="true" />
                : <CircleX size={14} aria-hidden="true" />}
            {responding ? "Sending response" : resolution?.status}
          </span>
        ) : queued > 0 ? <small className="permission-queue">{queued} more pending</small> : null}
      </header>
      {showBody ? (
        <div className="permission-body">
          {!terminal ? (
            relatedTool && taskId ? (
              <div className="permission-tool-context">
                <ActivityStepRow
                  onLoadToolImagePreview={onLoadToolImagePreview}
                  onSubscribeToolDetail={onSubscribeToolDetail}
                  step={relatedTool}
                  taskId={taskId}
                  toolDetails={toolDetails}
                />
              </div>
            ) : <PermissionToolSummary permission={permission} />
          ) : null}
          {(terminal ? display.description : permission.description) ? (
            <p>{terminal ? display.description : permission.description}</p>
          ) : null}
          {terminal && showCommand ? <code className="execute-command-chip">&gt;_ {display.chip}</code> : null}
          {showFacts ? (
            <dl className="permission-facts">
              {permission.scope ? (
                <>
                  <dt>
                    <Folder size={13} aria-hidden="true" />
                    Scope
                  </dt>
                  <dd>{permission.scope}</dd>
                </>
              ) : null}
              {permission.risk ? (
                <>
                  <dt>Risk</dt>
                  <dd>{permission.risk}</dd>
                </>
              ) : null}
            </dl>
          ) : null}
          {!terminal ? (
            <div className="permission-decision">
              <span className="permission-prompt">Choose how the Agent may continue</span>
              <div className="permission-actions" aria-label="Permission options">
                {permission.options.map((option) => (
                  <button
                    key={option.id}
                    className={`permission-option permission-option-${option.kind ?? "other"}`}
                    disabled={responding || !permissionDecisionForOption(option)}
                    onClick={(event) => respond(option, event.currentTarget)}
                    type="button"
                  >
                    <span className="permission-option-icon" aria-hidden="true">
                      {permissionOptionIcon(option.kind)}
                    </span>
                    <strong><PermissionOptionLabel label={option.label} /></strong>
                  </button>
                ))}
              </div>
            </div>
          ) : null}
          {response?.error ? <p className="permission-error" role="alert">{response.error}</p> : null}
        </div>
      ) : null}
    </section>
  );
}

function PermissionToolSummary({
  permission,
}: {
  permission: Extract<NormalizedMessage, { kind: "permission" }>;
}) {
  const title = (permission.tool_call.title || permission.title).trim() || "Tool call";
  return (
    <div className="permission-tool-summary">
      <span aria-hidden="true">{toolKindIcon(permission.tool_call.kind, 13)}</span>
      {permission.tool_call.kind === "execute"
        ? <code>{title}</code>
        : <span>{title}</span>}
    </div>
  );
}

function permissionResolutionLabel(
  permission: Extract<NormalizedMessage, { kind: "permission" }>,
  selected: PermissionOption | undefined,
) {
  if (permission.decision === "approved") {
    return selected
      ? { status: `Approved, ${selected.label}` }
      : { status: "Approved" };
  }
  if (permission.decision === "denied") {
    return selected
      ? { status: `Denied, ${selected.label}` }
      : { status: "Denied" };
  }
  return { status: permission.resolution_message ?? "Permission request cancelled" };
}

function permissionDisplay(permission: Extract<NormalizedMessage, { kind: "permission" }>) {
  const rawTitle = (permission.tool_call.title || permission.title).trim();
  const normalized = rawTitle.toLowerCase();
  const optionCommand = commandFromPermissionOptions(permission.options);
  if (isGenericToolCallTitle(rawTitle) && optionCommand) {
    return {
      title: "Approve command",
      description: permission.description ?? undefined,
      chip: optionCommand,
    };
  }
  if (normalized === "external_directory") {
    return {
      title: "External directory access",
      description:
        permission.description ?? "The Agent wants to access a directory outside the current workspace.",
      chip: permission.scope,
    };
  }
  if (permission.state === "pending" && permission.tool_call.kind === "execute" && rawTitle) {
    return {
      title: "Approve command",
      description: permission.description ?? undefined,
      chip: rawTitle,
    };
  }
  return {
    title: rawTitle || "Permission request",
    description:
      permission.description ?? (permission.title !== permission.tool_call.title ? permission.title : undefined),
    chip: rawTitle || undefined,
  };
}

function isGenericToolCallTitle(title: string) {
  return title.trim().toLowerCase() === "tool call";
}

function commandFromPermissionOptions(options: PermissionOption[]) {
  for (const option of options) {
    const backtickMatch = option.label.match(/`([^`]+)`/);
    if (backtickMatch?.[1]) return backtickMatch[1].trim();
  }
  return undefined;
}

export function permissionDecisionForOption(option: PermissionOption): "approved" | "denied" | undefined {
  if (option.kind === "allow_once" || option.kind === "allow_always") return "approved";
  if (option.kind === "reject_once" || option.kind === "reject_always") return "denied";
  return undefined;
}

function permissionOptionIcon(kind: PermissionOptionKind | undefined) {
  if (kind === "allow_once") return <Check size={14} />;
  if (kind === "allow_always") return <CheckCheck size={14} />;
  if (kind === "reject_once") return <X size={14} />;
  if (kind === "reject_always") return <ShieldX size={14} />;
  return <CircleAlert size={14} />;
}

function PermissionOptionLabel({ label }: { label: string }) {
  return label.split(/(`[^`]+`)/g).filter(Boolean).map((part, index) => (
    part.startsWith("`") && part.endsWith("`")
      ? <code key={`${part}:${index}`}>{part.slice(1, -1)}</code>
      : <span key={`${part}:${index}`}>{part}</span>
  ));
}
