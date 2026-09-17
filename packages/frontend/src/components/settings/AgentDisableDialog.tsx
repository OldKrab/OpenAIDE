import { PowerOff, X } from "lucide-react";

/**
 * Confirms disabling an Agent that has running Tasks. Disabling stops the Agent
 * process, so those Tasks are interrupted; the App Server rejects the change
 * without this acknowledgement.
 */
export function AgentDisableDialog({
  agentLabel,
  onCancel,
  onConfirm,
  runningTaskCount,
}: {
  agentLabel: string;
  onCancel: () => void;
  onConfirm: () => void;
  runningTaskCount: number;
}) {
  return (
    <div className="project-remove-backdrop">
      <section
        aria-label={`Disable ${agentLabel}`}
        aria-modal="true"
        className="project-remove-dialog"
        role="dialog"
      >
        <header>
          <span><PowerOff size={16} /></span>
          <button aria-label="Close" onClick={onCancel} type="button"><X size={15} /></button>
        </header>
        <h2>Disable {agentLabel}?</h2>
        <p>
          {runningTaskCount === 1 ? "1 running Task" : `${runningTaskCount} running Tasks`} for this
          Agent will be interrupted, and OpenAIDE will stop its process. Task history stays available.
        </p>
        <footer>
          <button onClick={onCancel} type="button">Cancel</button>
          <button className="danger" onClick={onConfirm} type="button">Disable Agent</button>
        </footer>
      </section>
    </div>
  );
}
