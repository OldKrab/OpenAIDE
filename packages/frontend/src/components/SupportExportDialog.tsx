import { AlertTriangle, Bug, CircleCheck, Download, ExternalLink } from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import {
  DIAGNOSTICS_CREATE_SUPPORT_EXPORT,
  DIAGNOSTICS_LIST_SUPPORT_EXPORT,
  type BackendConnection,
  type SupportExportListResult,
  type TaskId,
} from "@openaide/app-server-client";
import { currentFrontendShell } from "../services/frontendShell";
import { PopupDialog } from "./Popup";

const BUG_REPORT_URL = "https://github.com/OldKrab/OpenAIDE/issues/new?template=bug_report.yml";
const RECENT_SELECTION_WINDOW_MS = 15 * 60 * 1_000;

export function SupportExportButton({
  connection,
  taskId,
  compact = false,
}: {
  connection?: Pick<BackendConnection, "request">;
  taskId?: string;
  compact?: boolean;
}) {
  const shell = currentFrontendShell();
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState<"sessions" | "contents" | "saved">("sessions");
  const [catalog, setCatalog] = useState<SupportExportListResult>();
  const [selectedTasks, setSelectedTasks] = useState<Set<string>>(new Set());
  const [selectedUnbound, setSelectedUnbound] = useState<Set<string>>(new Set());
  const [includeHistory, setIncludeHistory] = useState(Boolean(taskId));
  const [includeTraces, setIncludeTraces] = useState(Boolean(taskId));
  const [includeNative, setIncludeNative] = useState(Boolean(taskId));
  const [includeSnapshot, setIncludeSnapshot] = useState(true);
  const [includeLogs, setIncludeLogs] = useState(true);
  const [pending, setPending] = useState(false);
  const [error, setError] = useState<string>();

  useEffect(() => {
    if (!open || !connection) return;
    setCatalog(undefined);
    setError(undefined);
    void connection.request(DIAGNOSTICS_LIST_SUPPORT_EXPORT, {
      ...(taskId ? { taskId: taskId as TaskId } : {}),
    }).then((result) => {
      setCatalog(result);
      const recentCutoff = Date.now() - RECENT_SELECTION_WINDOW_MS;
      const tasks = new Set(result.sessions
        .filter((session) => timestampMillis(session.lastActivity) >= recentCutoff)
        .map((session) => session.taskId));
      if (taskId && result.sessions.some((session) => session.taskId === taskId)) tasks.add(taskId as TaskId);
      setSelectedTasks(tasks);
      setSelectedUnbound(new Set(result.unboundTraces
        .filter((trace) => timestampMillis(trace.modifiedAt) >= recentCutoff)
        .map((trace) => trace.traceId)));
    }).catch(() => setError("Unable to load available diagnostic artifacts."));
  }, [connection, open, taskId]);

  const sensitive = (selectedTasks.size > 0 && (includeHistory || includeTraces || includeNative))
    || selectedUnbound.size > 0;
  const canExport = Boolean(connection && shell?.supportExports)
    && !pending
    && (includeSnapshot || includeLogs || sensitive);
  const selectedSessions = useMemo(() => catalog?.sessions.filter((session) => selectedTasks.has(session.taskId)) ?? [], [catalog, selectedTasks]);
  const unboundTraceGroups = useMemo(() => groupUnboundTraces(catalog), [catalog]);

  const openDialog = () => {
    const taskScoped = Boolean(taskId);
    setStep("sessions");
    setCatalog(undefined);
    setSelectedTasks(new Set());
    setSelectedUnbound(new Set());
    setIncludeHistory(taskScoped);
    setIncludeTraces(taskScoped);
    setIncludeNative(taskScoped);
    setIncludeSnapshot(true);
    setIncludeLogs(true);
    setPending(false);
    setError(undefined);
    setOpen(true);
  };

  const exportBundle = async () => {
    if (!connection || !shell?.supportExports) return;
    setPending(true);
    setError(undefined);
    try {
      const result = await connection.request(DIAGNOSTICS_CREATE_SUPPORT_EXPORT, {
        includeRuntimeSnapshot: includeSnapshot,
        includeLogs,
        sessions: selectedSessions.map((session) => ({
          taskId: session.taskId,
          includeOpenaideHistory: includeHistory,
          includeAcpTraces: includeTraces,
          includeNativeTranscript: includeNative && session.nativeTranscript === "available",
        })),
        unboundTraceIds: [...selectedUnbound],
      });
      const outcome = await shell.supportExports.save({ fileHandleId: result.fileHandleId, label: result.label });
      if (outcome === "cancelled") return;
      setStep("saved");
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : "Unable to export diagnostics.");
    } finally {
      setPending(false);
    }
  };

  return (
    <>
      <button
        className={compact ? "task-support-export-trigger" : "general-support-export"}
        disabled={!connection || !shell?.supportExports}
        onClick={openDialog}
        type="button"
      >
        <Bug size={14} />{compact ? "Diagnostics" : "Export diagnostics…"}
      </button>
      <PopupDialog className={`settings-reset-dialog support-export-dialog${step === "saved" ? " support-export-dialog-saved" : ""}`} label="Export diagnostics" onOpenChange={setOpen} open={open}>
        <header><Bug size={18} /><div><strong>Export diagnostics</strong></div></header>
        {step !== "saved" ? <section className="support-export-step-heading">
          <div><small>Step {step === "sessions" ? "1" : "2"} of 2</small><progress aria-label={`Export progress, step ${step === "sessions" ? "1" : "2"} of 2`} max={2} value={step === "sessions" ? 1 : 2} /></div>
          <h2>{step === "sessions" ? "Choose relevant sessions" : "Choose bundle contents"}</h2>
          <p>{step === "sessions" ? "Activity from the last 15 minutes is preselected. Adjust it to match the problem you saw." : "Standard diagnostics are safe by default. Session sources are raw and may be sensitive."}</p>
        </section> : null}
        {!catalog && !error ? <div className="support-export-content"><p role="status">Loading available artifacts…</p></div> : null}
        {catalog ? (
          <div className="support-export-content">
            {step === "sessions" ? (
              <>
                <fieldset className="support-export-session-fieldset">
                  <legend>Sessions</legend>
                  {catalog.sessions.map((session) => (
                    <SessionCheck
                      key={session.taskId}
                      checked={selectedTasks.has(session.taskId)}
                      onChange={(checked) => setSelectedTasks(toggle(selectedTasks, session.taskId, checked))}
                      session={session}
                    />
                  ))}
                  {catalog.sessions.length === 0 ? <small>No saved Agent sessions are available.</small> : null}
                </fieldset>
                {catalog.unboundTraces.length > 0 ? (
                  <fieldset>
                    <legend>Unbound failed traces</legend>
                    {unboundTraceGroups.map(([group, traces]) => (
                      <section className="support-export-trace-group" key={group}>
                        <small>{group}</small>
                        {traces.map((trace) => (
                          <Check key={trace.traceId} checked={selectedUnbound.has(trace.traceId)} label={`${trace.operation} · ${formatTimestamp(trace.modifiedAt)} · ${formatBytes(trace.sizeBytes)}`} onChange={(checked) => setSelectedUnbound(toggle(selectedUnbound, trace.traceId, checked))} />
                        ))}
                      </section>
                    ))}
                  </fieldset>
                ) : null}
              </>
            ) : step === "contents" ? (
              <>
                <p className="support-export-selection-summary">{selectionSummary(selectedTasks.size, selectedUnbound.size)}</p>
                <fieldset>
                  <legend>Standard diagnostics</legend>
                  <Check label="Runtime snapshot" checked={includeSnapshot} onChange={setIncludeSnapshot} />
                  <Check label="Recent App Server and shell logs" checked={includeLogs} onChange={setIncludeLogs} />
                </fieldset>
                <fieldset>
                  <legend>Selected session sources</legend>
                  <Check disabled={selectedTasks.size === 0} label="OpenAIDE history" checked={includeHistory} onChange={setIncludeHistory} />
                  <Check disabled={selectedTasks.size === 0} label="Associated ACP traces" checked={includeTraces} onChange={setIncludeTraces} />
                  <Check disabled={selectedTasks.size === 0} label="Native Agent transcript when available" checked={includeNative} onChange={setIncludeNative} />
                  {!catalog.acpTraceEnabled ? <small>ACP tracing is off. Existing traces remain selectable; new problems require tracing to be enabled before reproduction.</small> : null}
                </fieldset>
                {sensitive ? (
                  <p className="support-export-warning" role="note"><AlertTriangle size={16} />Selected raw artifacts may contain prompts, responses, paths, tool output, and secrets. Review the ZIP before sharing it.</p>
                ) : null}
              </>
            ) : (
              <div className="support-export-saved" role="status">
                <CircleCheck size={22} />
                <div><h2>Diagnostics saved</h2><p>The ZIP is ready. Review it before sharing if you included raw session sources.</p></div>
              </div>
            )}
            {error ? <p className="settings-reset-error" role="alert">{error}</p> : null}
          </div>
        ) : null}
        <footer>
          {step === "sessions" ? (
            <>
              <button disabled={pending} onClick={() => setOpen(false)} type="button">Cancel</button>
              <button disabled={!catalog} onClick={() => setStep("contents")} type="button">Continue</button>
            </>
          ) : step === "contents" ? (
            <>
              <button disabled={pending} onClick={() => setStep("sessions")} type="button">Back</button>
              <button disabled={!canExport} onClick={() => void exportBundle()} type="button"><Download size={14} />{pending ? "Exporting…" : "Export"}</button>
            </>
          ) : (
            <>
              <button onClick={() => shell?.recovery.openExternal(BUG_REPORT_URL)} type="button"><ExternalLink size={14} />Open GitHub bug report</button>
              <button onClick={() => setOpen(false)} type="button">Done</button>
            </>
          )}
        </footer>
      </PopupDialog>
    </>
  );
}

function Check({ checked, disabled = false, label, onChange }: { checked: boolean; disabled?: boolean; label: string; onChange(checked: boolean): void }) {
  return <label className="support-export-check"><input checked={checked} disabled={disabled} onChange={(event) => onChange(event.currentTarget.checked)} type="checkbox" /><span>{label}</span></label>;
}

function SessionCheck({
  checked,
  onChange,
  session,
}: {
  checked: boolean;
  onChange(checked: boolean): void;
  session: SupportExportListResult["sessions"][number];
}) {
  return (
    <label className="support-export-session-row">
      <input checked={checked} onChange={(event) => onChange(event.currentTarget.checked)} type="checkbox" />
      <span className="support-export-session-identity">
        <strong title={session.title}>{session.title}</strong>
        <span className="support-export-session-meta">
          <span>{session.agentName}</span>
          <span>{session.projectLabel}</span>
          <time dateTime={session.lastActivity}>{formatTimestamp(session.lastActivity)}</time>
        </span>
      </span>
      <span className="support-export-session-status">
        {session.active ? <small>Active</small> : null}
        <small>{session.acpTraceCount} trace{session.acpTraceCount === 1 ? "" : "s"}</small>
        {session.nativeTranscript === "available" ? <small>Transcript available</small> : null}
      </span>
    </label>
  );
}

function toggle(values: Set<string>, value: string, enabled: boolean) {
  const next = new Set(values);
  if (enabled) next.add(value); else next.delete(value);
  return next;
}

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${Math.ceil(bytes / 1024)} KiB`;
  return `${(bytes / (1024 * 1024)).toFixed(1)} MiB`;
}

function formatTimestamp(value: string) {
  const date = new Date(timestampMillis(value));
  return Number.isNaN(date.getTime()) ? value : date.toLocaleString();
}

function timestampMillis(value: string) {
  const timestamp = Number(value);
  return Number.isFinite(timestamp) ? timestamp : new Date(value).getTime();
}

function groupUnboundTraces(catalog: SupportExportListResult | undefined) {
  const groups = new Map<string, SupportExportListResult["unboundTraces"]>();
  for (const trace of catalog?.unboundTraces ?? []) {
    const group = trace.taskId ? `Task ${trace.taskId}` : "Unknown task";
    groups.set(group, [...(groups.get(group) ?? []), trace]);
  }
  return [...groups.entries()];
}

function selectionSummary(sessionCount: number, traceCount: number) {
  if (sessionCount === 0 && traceCount === 0) return "No sessions selected. The bundle will contain standard diagnostics only.";
  const parts = [];
  if (sessionCount > 0) parts.push(`${sessionCount} session${sessionCount === 1 ? "" : "s"}`);
  if (traceCount > 0) parts.push(`${traceCount} unbound trace${traceCount === 1 ? "" : "s"}`);
  return `${parts.join(" and ")} selected.`;
}
