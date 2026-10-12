import { Copy, LifeBuoy } from "lucide-react";
import { useState } from "react";
import type { BackendConnection } from "@openaide/app-server-client";
import type { RuntimeSettingsResult } from "@openaide/app-shell-contracts";
import { copyText } from "../clipboard";
import { SupportExportButton } from "../SupportExportDialog";
import type { PhoneConnection } from "./phoneConnection";
import { settingsInfo } from "./settingsInfo";
import { SettingsButton, SettingsConfirm, SettingsInlineError, SettingsNotice, SettingsSwitch } from "./settingsKit";
import type { SettingsGroup, SettingsRow, SettingsSection } from "./settingsModel";

export type TroubleshootingSectionProps = {
  backendConnection?: Pick<BackendConnection, "request">;
  onResetTaskHistory?: () => Promise<void>;
  onSetAcpTrace: (enabled: boolean) => void;
  phone?: PhoneConnection;
  runtimeSettings?: RuntimeSettingsResult;
  supportExportRequestKey?: string;
};

/** Where a user goes when something is wrong: the export first, then checks, then the tools that dig deeper. */
export function useTroubleshootingSection({
  backendConnection,
  onResetTaskHistory,
  onSetAcpTrace,
  phone,
  runtimeSettings,
  supportExportRequestKey,
}: TroubleshootingSectionProps): SettingsSection {
  const [resetting, setResetting] = useState(false);
  const [copied, setCopied] = useState(false);
  const developer = runtimeSettings?.developer;
  const support: SettingsRow[] = [{
    id: "export-logs",
    label: "Export logs",
    info: settingsInfo.exportLogs,
    description: "Redacted diagnostics to attach to a bug report",
    keywords: "support diagnostics help problem bundle troubleshooting bug report",
    control: <SupportExportButton connection={backendConnection} openRequestKey={supportExportRequestKey} />,
  }];
  if (phone) {
    support.push({
      id: "phone-report",
      label: "Phone connection report",
      description: "No conversations or credentials",
      keywords: "diagnostics share connection",
      control: <SettingsButton disabled={phone.busy} onClick={() => void phone.execute({ action: "diagnostics" })}>Share</SettingsButton>,
    });
  }

  const groups: SettingsGroup[] = [{ id: "support", rows: support }];
  const setup = phoneSetupGroup(phone);
  if (setup) groups.push(setup);
  if (developer) {
    groups.push({
      id: "developer",
      title: "Developer",
      rows: [
        {
          id: "acp-logs",
          label: "ACP logs",
          description: "Write protocol traces to disk",
          keywords: "trace debug developer",
          control: <SettingsSwitch checked={developer.acp_trace.enabled} label="ACP logs" onChange={onSetAcpTrace} />,
        },
        {
          id: "trace-folder",
          label: "Trace folder",
          keywords: "path directory acp",
          control: (
            <>
              <code className="settings-mono" title={developer.acp_trace.directory}>{compactPath(developer.acp_trace.directory)}</code>
              <SettingsButton
                ariaLabel="Copy trace folder path"
                icon={<Copy aria-hidden="true" size={13} />}
                onClick={() => { void copyText(developer.acp_trace.directory).then(() => setCopied(true), () => undefined); }}
                variant="quiet"
              >
                {copied ? "Copied" : "Copy"}
              </SettingsButton>
            </>
          ),
        },
      ],
    });
  }
  if (onResetTaskHistory) {
    groups.push({
      id: "data",
      title: "Data",
      rows: [{
        id: "task-history",
        label: "Task history",
        info: settingsInfo.reset,
        description: "Tasks, chats and recalled prompts on this computer",
        keywords: "reset delete chats local data",
        control: <SettingsButton ariaLabel="Reset task history" danger onClick={() => setResetting(true)}>Reset…</SettingsButton>,
      }],
    });
  }

  return {
    id: "troubleshooting",
    label: "Troubleshooting",
    icon: LifeBuoy,
    groups,
    overlay: resetting && onResetTaskHistory ? (
      <SettingsConfirm
        confirmLabel="Reset task history"
        detail="Every Task, chat, tool detail and recalled prompt on this computer is deleted. Projects, files and settings are kept. This cannot be undone."
        label="Reset task history confirmation"
        onClose={() => setResetting(false)}
        onConfirm={onResetTaskHistory}
        pendingLabel="Resetting…"
        title="Reset task history?"
      />
    ) : undefined,
  };
}

/** Phone only: what the phone still needs before it can run Agents itself. */
function phoneSetupGroup(phone: PhoneConnection | undefined): SettingsGroup | undefined {
  const state = phone?.state;
  if (!phone || !state || state.remote) return undefined;
  const checks = state.checks;
  const needsTools = Boolean(checks && ["node", "nodeVersion", "git", "npm", "agent", "runtime"].some((key) => !checks[key]));
  const act = (label: string, action: "check" | "get_termux" | "grant" | "install" | "termux" | "signin") => (
    <SettingsButton disabled={phone.busy} onClick={() => void phone.execute({ action })}>{label}</SettingsButton>
  );
  const ready = Boolean(checks && !needsTools && checks.authenticated && checks.agentVersion);
  const rows: SettingsRow[] = [{
    id: "check",
    label: "Phone setup",
    keywords: "termux tools install check local",
    status: ready ? { tone: "ok", text: "Ready for local work" } : undefined,
    control: act("Check", "check"),
  }];
  if (!state.termux) rows.push({ id: "termux", label: "Termux", description: "Not installed", control: act("Get Termux", "get_termux") });
  else if (!state.permission) rows.push({ id: "termux-access", label: "Termux access", description: "OpenAIDE runs Agents through Termux", control: act("Allow", "grant") });
  if (needsTools) rows.push({ id: "tools", label: "Required tools", description: "Your projects are kept", control: act("Install", "install") });
  if (checks && !needsTools && !checks.claude) rows.push({ id: "claude", label: "Claude", description: "Not installed on this phone", control: act("Install", "install") });
  if (checks && checks.agent && !checks.agentVersion) rows.push({ id: "agent-version", label: "Agent version", description: "Requires Codex 0.153.3", control: act("Open Termux", "termux") });
  if (checks && !checks.authenticated) rows.push({ id: "sign-in", label: "Agent sign-in", description: "Paste the copied command in Termux", control: act("Sign in", "signin") });
  return {
    id: "phone-setup",
    title: "This phone",
    notice: (
      <>
        {state.notice ? <SettingsNotice>{state.notice}</SettingsNotice> : null}
        {phone.error ? <SettingsInlineError>{phone.error}</SettingsInlineError> : null}
      </>
    ),
    rows,
  };
}

/** Keeps the end of a long path, which is the part that tells folders apart. */
export function compactPath(path: string, visibleSegments = 3): string {
  if (!path) return path;
  const parts = path.split(/[\\/]+/).filter(Boolean);
  if (parts.length <= visibleSegments) return path;
  const separator = path.includes("\\") ? "\\" : "/";
  return `...${separator}${parts.slice(-visibleSegments).join(separator)}`;
}
