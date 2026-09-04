import { useEffect, useState } from "react";
import {
  ArrowLeft,
  ChevronRight,
  CircleAlert,
  Copy,
  ExternalLink,
  KeyRound,
  LoaderCircle,
  Save,
  Terminal,
} from "lucide-react";
import type { AgentSettingsRecord, AgentSignInFlowRecord } from "@openaide/app-shell-contracts";
import { authMethodUsesValues, type AgentAuthMethod } from "./agentSettingsModel";

type AuthenticateAgent = (agentId: string, methodId: string, values?: Record<string, string>) => void;

/**
 * Agent Sign-in surface. Renders the App Server-owned Sign-in Flow: while no flow exists the user
 * picks a method; once a flow exists the surface collapses to that flow's single next step. All
 * state comes from `agent.sign_in`; this component never predicts what the server will do.
 */
export function AgentSignIn({
  agent,
  onAuthenticate,
  onCancel,
  onLogout,
}: {
  agent: AgentSettingsRecord;
  onAuthenticate: AuthenticateAgent;
  onCancel?: (agentId: string) => void | Promise<void>;
  onLogout?: (agentId: string) => boolean | void | Promise<boolean | void>;
}) {
  const flow = agent.sign_in;
  const isRequired = agent.status === "auth_required";
  const [panel, setPanel] = useState<"summary" | "methods">("summary");
  const [selectedMethodId, setSelectedMethodId] = useState<string>();
  const [logoutPending, setLogoutPending] = useState(false);
  const selectedMethod = agent.auth_methods.find((method) => method.id === selectedMethodId);
  const authenticate: AuthenticateAgent = (agentId, methodId, values) => {
    setPanel("summary");
    setSelectedMethodId(undefined);
    if (values === undefined) {
      onAuthenticate(agentId, methodId);
    } else {
      onAuthenticate(agentId, methodId, values);
    }
  };
  const cancel = onCancel ? () => {
    void Promise.resolve(onCancel(agent.id)).then(() => {
      setPanel("methods");
      setSelectedMethodId(undefined);
    });
  } : undefined;
  const canLogout = agent.status === "connected" && agent.logout_supported && onLogout;
  const logout = canLogout ? async () => {
    setLogoutPending(true);
    try {
      await onLogout(agent.id);
    } finally {
      setLogoutPending(false);
    }
  } : undefined;
  return (
    <section aria-label={isRequired ? "Sign in" : "Authentication"} className="agent-page-section agent-sign-in">
      {flow ? (
        <AgentSignInStep agent={agent} flow={flow} onAuthenticate={authenticate} onCancel={cancel} />
      ) : selectedMethod ? (
        <AgentSignInValueStep
          agentId={agent.id}
          method={selectedMethod}
          onAuthenticate={authenticate}
          onBack={() => setSelectedMethodId(undefined)}
        />
      ) : panel === "methods" ? (
        <AgentSignInMethods
          methods={agent.auth_methods}
          onBack={() => setPanel("summary")}
          onSelect={(method) => {
            if (authMethodUsesValues(method)) {
              setSelectedMethodId(method.id);
              return;
            }
            authenticate(agent.id, method.id);
          }}
        />
      ) : (
        <div className={`agent-page-surface agent-sign-in-summary${isRequired ? " attention" : ""}`}>
          <span className="agent-sign-in-step-icon"><KeyRound aria-hidden="true" size={18} /></span>
          <span className="agent-sign-in-summary-copy">
            <strong>{isRequired
              ? "Sign in required"
              : "Authentication"}</strong>
          </span>
          <span className="agent-sign-in-summary-actions">
            <button className={`agent-page-row-button${isRequired ? " primary" : ""}`} type="button" onClick={() => setPanel("methods")}>
              <span>{isRequired ? "Choose method" : "Manage"}</span>
              <ChevronRight aria-hidden="true" size={13} />
            </button>
            {logout ? (
              <button
                aria-busy={logoutPending || undefined}
                className="agent-page-row-button danger"
                disabled={logoutPending || agent.logout_blocked_by_running_task}
                title={agent.logout_blocked_by_running_task ? "Stop running Tasks before signing out." : undefined}
                type="button"
                onClick={() => void logout()}
              >
                {logoutPending ? "Signing out…" : "Sign out"}
              </button>
            ) : null}
          </span>
        </div>
      )}
    </section>
  );
}

function AgentSignInMethods({
  methods,
  onBack,
  onSelect,
}: {
  methods: AgentAuthMethod[];
  onBack: () => void;
  onSelect: (method: AgentAuthMethod) => void;
}) {
  return (
    <div className="agent-page-surface agent-sign-in-chooser">
      <div className="agent-sign-in-panel-heading">
        <span>
          <strong>Choose a method</strong>
        </span>
        <button className="agent-sign-in-secondary" type="button" onClick={onBack}>
          <ArrowLeft aria-hidden="true" size={13} />
          <span>Back</span>
        </button>
      </div>
      <div className="agent-sign-in-methods">
        {methods.map((method) => (
          <button className="agent-sign-in-method" key={method.id} type="button" onClick={() => onSelect(method)}>
            <span className="agent-sign-in-method-icon">{authMethodIcon(method)}</span>
            <span className="agent-sign-in-method-copy">
              <strong>{method.label}</strong>
              {method.description ? <small>{method.description}</small> : null}
            </span>
            <ChevronRight aria-hidden="true" size={15} />
          </button>
        ))}
      </div>
    </div>
  );
}

function methodLabel(agent: AgentSettingsRecord, methodId: string) {
  return agent.auth_methods.find((method) => method.id === methodId)?.label ?? methodId;
}

/** One focused step for the running or failed flow, with Cancel always one click away. */
function AgentSignInStep({
  agent,
  flow,
  onAuthenticate,
  onCancel,
}: {
  agent: AgentSettingsRecord;
  flow: AgentSignInFlowRecord;
  onAuthenticate: AuthenticateAgent;
  onCancel?: () => void;
}) {
  const label = methodLabel(agent, flow.method_id);
  const cancelButton = onCancel ? (
    <button className="agent-sign-in-secondary" type="button" onClick={onCancel}>Cancel sign-in</button>
  ) : null;
  switch (flow.phase) {
    case "starting":
      return (
        <div aria-busy="true" className="agent-page-surface agent-sign-in-step" role="status">
          <span className="agent-sign-in-step-icon"><LoaderCircle aria-hidden="true" className="spin" size={18} /></span>
          <div className="agent-sign-in-step-copy">
            <strong>Starting {label}…</strong>
            <p>Waiting for {agent.label} to begin sign-in.</p>
          </div>
          <div className="agent-sign-in-step-actions">{cancelButton}</div>
        </div>
      );
    case "awaiting_user":
      return (
        <div className="agent-page-surface agent-sign-in-step" role="status">
          <span className="agent-sign-in-step-icon"><ExternalLink aria-hidden="true" size={18} /></span>
          <div className="agent-sign-in-step-copy">
            <strong>Continue in your browser</strong>
            {flow.hint ? <p className="agent-sign-in-hint">{flow.hint}</p> : <p>Open the sign-in page and finish there. This page updates when {agent.label} confirms.</p>}
            {flow.url ? <p className="agent-sign-in-url">{flow.url}</p> : null}
          </div>
          <div className="agent-sign-in-step-actions">
            {flow.url ? (
              <a className="agent-page-row-button primary" href={flow.url} rel="noopener noreferrer" target="_blank">
                <ExternalLink size={13} />
                <span>Open sign-in page</span>
              </a>
            ) : null}
            {flow.url ? <CopyLinkButton url={flow.url} /> : null}
            {cancelButton}
          </div>
        </div>
      );
    case "awaiting_terminal":
      return (
        <div className="agent-page-surface agent-sign-in-step" role="status">
          <span className="agent-sign-in-step-icon"><Terminal aria-hidden="true" size={18} /></span>
          <div className="agent-sign-in-step-copy">
            <strong>Finish in the terminal</strong>
            <p>A terminal opened for {label}. Come back here once it reports you are signed in.</p>
          </div>
          <div className="agent-sign-in-step-actions">
            <button className="agent-page-row-button primary" type="button" onClick={() => onAuthenticate(agent.id, flow.method_id)}>
              <Terminal size={13} />
              <span>I've finished signing in</span>
            </button>
            {cancelButton}
          </div>
        </div>
      );
    case "failed":
      return (
        <div className="agent-page-surface attention agent-sign-in-step" role="alert">
          <span className="agent-sign-in-step-icon failed"><CircleAlert aria-hidden="true" size={18} /></span>
          <div className="agent-sign-in-step-copy">
            <strong>Sign-in didn't complete</strong>
            <p>{flow.failure ?? `${agent.label} could not sign in with ${label}.`}</p>
          </div>
          <div className="agent-sign-in-step-actions">
            <button className="agent-page-row-button primary" type="button" onClick={() => onAuthenticate(agent.id, flow.method_id)}>
              <span>Try again</span>
            </button>
            {onCancel ? (
              <button className="agent-sign-in-secondary" type="button" onClick={onCancel}>Choose another method</button>
            ) : null}
          </div>
        </div>
      );
  }
}

function CopyLinkButton({ url }: { url: string }) {
  const [copied, setCopied] = useState(false);
  useEffect(() => {
    if (!copied) return;
    const timer = setTimeout(() => setCopied(false), 1500);
    return () => clearTimeout(timer);
  }, [copied]);
  if (typeof navigator === "undefined" || !navigator.clipboard) return null;
  return (
    <button
      className="agent-page-row-button"
      type="button"
      onClick={() => {
        void navigator.clipboard.writeText(url).then(() => setCopied(true), () => undefined);
      }}
    >
      <Copy size={13} />
      <span>{copied ? "Copied" : "Copy link"}</span>
    </button>
  );
}

/** Focused value step. Secrets remain write-only and exist only in this ephemeral form state. */
function AgentSignInValueStep({
  agentId,
  method,
  onAuthenticate,
  onBack,
}: {
  agentId: string;
  method: AgentAuthMethod;
  onAuthenticate: AuthenticateAgent;
  onBack: () => void;
}) {
  const [values, setValues] = useState<Record<string, string>>({});
  const variables = method.variables ?? [];
  const missingRequired = variables.some((variable) => !variable.optional && !(values[variable.name] ?? "").trim());
  const submit = () => {
    if (!missingRequired) onAuthenticate(agentId, method.id, values);
  };
  return (
    <form
      className="agent-page-surface agent-sign-in-value-step"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <div className="agent-sign-in-panel-heading">
        <span>
          <strong>Enter your {method.label}</strong>
          <small>{method.description ?? "The value is stored using this App Shell's protected credential storage."}</small>
        </span>
        <button className="agent-sign-in-secondary" type="button" onClick={onBack}>
          <ArrowLeft aria-hidden="true" size={13} />
          <span>Back</span>
        </button>
      </div>
      <div className="agent-sign-in-fields">
        {variables.map((variable) => (
          <label key={variable.name}>
            <span>{variable.label ?? variable.name}{variable.optional ? " (optional)" : ""}</span>
            <input
              aria-label={variable.label ?? variable.name}
              autoComplete="off"
              type={variable.secret ? "password" : "text"}
              value={values[variable.name] ?? ""}
              onChange={(event) => {
                const value = event.currentTarget.value;
                setValues((current) => ({ ...current, [variable.name]: value }));
              }}
              onKeyDown={(event) => {
                if (event.key !== "Enter") return;
                event.preventDefault();
                submit();
              }}
            />
          </label>
        ))}
      </div>
      <div className="agent-sign-in-value-actions">
        {method.link ? <a href={method.link} rel="noopener noreferrer" target="_blank">Get a key</a> : <span />}
        <button
          className="agent-page-row-button primary"
          disabled={missingRequired}
          type="submit"
        >
          <Save aria-hidden="true" size={13} />
          <span>Save</span>
        </button>
      </div>
    </form>
  );
}

function authMethodIcon(method: AgentAuthMethod) {
  if (method.kind === "env_var") return <KeyRound aria-hidden="true" size={17} />;
  if (method.kind === "terminal") return <Terminal aria-hidden="true" size={17} />;
  return <ExternalLink aria-hidden="true" size={17} />;
}
