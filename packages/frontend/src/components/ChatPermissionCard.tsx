import { Check, CircleX, Clock3, Folder, LoaderCircle } from "lucide-react";
import type { NormalizedMessage, PermissionOption } from "@openaide/app-shell-contracts";
import { toolKindClass } from "../state/toolDetailsViewModel";
import { toolKindIcon } from "./chatToolIcons";

export function ChatPermissionCard({
  onRespond,
  permission,
  response,
}: {
  permission: Extract<NormalizedMessage, { kind: "permission" }>;
  response?: { responding: boolean; error?: string };
  onRespond: (
    requestId: string,
    optionId: string,
    decision: "approved" | "denied",
    source?: "agent" | "appServer",
  ) => void;
}) {
  const selected = permission.options.find((option) => option.id === permission.selected_option);
  const terminal = permission.state === "resolved" || permission.state === "cancelled";
  const approved = permission.state === "resolved" && permission.decision === "approved";
  const responding = response?.responding ?? false;
  const resolution = terminal ? permissionResolutionLabel(permission, selected) : undefined;
  const statusLabel = terminal ? resolution?.status : responding ? "Sending response" : "Waiting";
  const display = permissionDisplay(permission);
  const showCommand = Boolean(display.chip && display.chip !== display.title);
  const showFacts = Boolean(permission.scope || permission.risk);
  const showBody = Boolean(display.description || showCommand || showFacts || response?.error || !terminal);

  const respond = (option: PermissionOption) => {
    if (responding || terminal) return;
    const decision = permissionDecisionForOption(option);
    if (!decision) return;
    onRespond(
      permission.app_server_request_id ?? permission.request_id,
      option.id,
      decision,
      permission.app_server_request_id ? "appServer" : "agent",
    );
  };

  return (
    <section
      className={`permission-card tool-${toolKindClass(permission.tool_call.kind ?? "other")} ${terminal ? "resolved" : ""}`}
      aria-label="Permission request"
    >
      <header className="permission-head">
        <span className="permission-icon" aria-hidden="true">
          {toolKindIcon(permission.tool_call.kind, 14)}
        </span>
        <span className="permission-title">
          <strong>{display.title}</strong>
        </span>
        <span
          className={`permission-state ${
            responding ? "responding" : terminal ? (approved ? "approved" : permission.state === "cancelled" ? "cancelled" : "denied") : "waiting"
          }`}
        >
          {responding ? (
            <LoaderCircle size={14} aria-hidden="true" />
          ) : terminal ? (
            approved ? (
              <Check size={14} aria-hidden="true" />
            ) : (
              <CircleX size={14} aria-hidden="true" />
            )
          ) : (
            <Clock3 size={14} aria-hidden="true" />
          )}
          {terminal || responding ? statusLabel : "Approval required"}
        </span>
      </header>
      {showBody ? (
        <div className="permission-body">
          {display.description ? <p>{display.description}</p> : null}
          {showCommand ? <code className="execute-command-chip">&gt;_ {display.chip}</code> : null}
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
            <div className="permission-actions" aria-label="Permission options">
              {permission.options.map((option) => (
                <button
                  key={option.id}
                  className={option.kind === "deny" ? "deny" : option.id.includes("amendment") ? "remember" : "allow"}
                  disabled={responding || !permissionDecisionForOption(option)}
                  onClick={() => respond(option)}
                  type="button"
                >
                  {option.label}
                </button>
              ))}
            </div>
          ) : null}
          {response?.error ? <p className="permission-error">{response.error}</p> : null}
        </div>
      ) : null}
    </section>
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
  return { status: "Permission request cancelled" };
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
  if (option.kind === "allow") return "approved";
  if (option.kind === "deny") return "denied";
  return undefined;
}
