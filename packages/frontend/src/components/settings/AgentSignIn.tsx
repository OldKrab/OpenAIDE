import { lazy, Suspense, useEffect, useState, useSyncExternalStore } from "react";
import { ArrowLeft, ChevronRight, Copy, ExternalLink, KeyRound, LoaderCircle, Terminal } from "lucide-react";
import type { AgentSettingsRecord, AgentSignInFlowRecord } from "@openaide/app-shell-contracts";
import { authMethodUsesValues, type AgentAuthMethod } from "./agentSettingsModel";
import { authTerminalForAgent, subscribeAuthTerminals } from "../../services/authTerminals";
import { SettingsButton, SettingsField, SettingsRowView } from "./settingsKit";
const AgentAuthTerminal = lazy(() => import("./AgentAuthTerminal").then((module) => ({ default: module.AgentAuthTerminal })));

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
  const terminal = useSyncExternalStore(subscribeAuthTerminals, () => authTerminalForAgent(agent.id));
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
    <section aria-label={isRequired ? "Sign in" : "Authentication"} className="settings-group">
      <header><h2>Account</h2></header>
      {terminal && flow && flow.phase !== "failed" ? (
        <Suspense fallback={<p className="settings-empty" role="status">Opening sign-in terminal…</p>}>
          <AgentAuthTerminal session={terminal} onCancel={cancel} />
        </Suspense>
      ) : flow ? (
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
        <ul className="settings-card">
          <SettingsRowView
            row={{
              id: "sign-in",
              label: isRequired ? "Sign in required" : agent.status === "connected" ? "Signed in" : "Sign-in",
              leading: <KeyRound aria-hidden="true" size={18} strokeWidth={1.6} />,
              control: (
                <>
                  {logout ? (
                    <SettingsButton
                      busy={logoutPending}
                      disabled={agent.logout_blocked_by_running_task}
                      onClick={() => void logout()}
                      title={agent.logout_blocked_by_running_task ? "Stop running Tasks before signing out." : undefined}
                      variant="quiet"
                    >
                      {logoutPending ? "Signing out…" : "Sign out"}
                    </SettingsButton>
                  ) : null}
                  <SettingsButton onClick={() => setPanel("methods")} variant={isRequired ? "primary" : "default"}>
                    {isRequired ? "Choose method" : "Manage"}
                  </SettingsButton>
                </>
              ),
            }}
          />
        </ul>
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
    <div className="settings-card settings-step">
      <header>
        <strong>Choose a method</strong>
        <SettingsButton icon={<ArrowLeft aria-hidden="true" size={13} />} onClick={onBack} variant="quiet">Back</SettingsButton>
      </header>
      <ul>
        {methods.map((method) => (
          <li className="settings-row" key={method.id}>
            <div className="settings-row-line">
              <button className="settings-row-main" onClick={() => onSelect(method)} type="button">
                <span className="settings-row-leading">{authMethodIcon(method)}</span>
                <span className="settings-row-copy">
                  <strong>{method.label}</strong>
                  {method.description ? <small>{method.description}</small> : null}
                </span>
              </button>
              <span className="settings-row-end"><ChevronRight aria-hidden="true" className="settings-row-chevron" size={16} /></span>
            </div>
          </li>
        ))}
      </ul>
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
  const cancelButton = onCancel ? <SettingsButton onClick={onCancel} variant="quiet">Cancel sign-in</SettingsButton> : null;
  switch (flow.phase) {
    case "starting":
      return (
        <div aria-busy="true" className="settings-card settings-step" role="status">
          <div className="settings-step-copy">
            <strong><LoaderCircle aria-hidden="true" className="spin" size={15} />Starting {label}…</strong>
            <p>Waiting for {agent.label} to begin sign-in.</p>
          </div>
          <div className="settings-step-actions">{cancelButton}</div>
        </div>
      );
    case "awaiting_user":
      return (
        <div className="settings-card settings-step" role="status">
          <div className="settings-step-copy">
            <strong>Continue in your browser</strong>
            {flow.hint ? <p className="settings-step-hint">{flow.hint}</p> : <p>Open the sign-in page and finish there. This page updates when {agent.label} confirms.</p>}
            {flow.url ? <p className="settings-step-url">{flow.url}</p> : null}
          </div>
          <div className="settings-step-actions">
            {flow.url ? (
              <a className="settings-action primary" href={flow.url} rel="noopener noreferrer" target="_blank">
                <ExternalLink aria-hidden="true" size={13} />
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
        <div className="settings-card settings-step" role="status">
          <div className="settings-step-copy">
            <strong>Finish in the terminal</strong>
            <p>A terminal opened for {label}. Come back here once it reports you are signed in.</p>
          </div>
          <div className="settings-step-actions">
            <SettingsButton icon={<Terminal aria-hidden="true" size={13} />} onClick={() => onAuthenticate(agent.id, flow.method_id)} variant="primary">
              I've finished signing in
            </SettingsButton>
            {cancelButton}
          </div>
        </div>
      );
    case "failed":
      return (
        <div className="settings-card settings-step warn" role="alert">
          <div className="settings-step-copy">
            <strong>Sign-in didn't complete</strong>
            <p>{flow.failure ?? `${agent.label} could not sign in with ${label}.`}</p>
          </div>
          <div className="settings-step-actions">
            <SettingsButton onClick={() => onAuthenticate(agent.id, flow.method_id)} variant="primary">Try again</SettingsButton>
            {onCancel ? <SettingsButton onClick={onCancel} variant="quiet">Choose another method</SettingsButton> : null}
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
    <SettingsButton
      icon={<Copy aria-hidden="true" size={13} />}
      onClick={() => {
        void navigator.clipboard.writeText(url).then(() => setCopied(true), () => undefined);
      }}
    >
      {copied ? "Copied" : "Copy link"}
    </SettingsButton>
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
      className="settings-card settings-step"
      onSubmit={(event) => {
        event.preventDefault();
        submit();
      }}
    >
      <header>
        <span className="settings-step-copy">
          <strong>Enter your {method.label}</strong>
          <p>{method.description ?? "The value is stored using this App Shell's protected credential storage."}</p>
        </span>
        <SettingsButton icon={<ArrowLeft aria-hidden="true" size={13} />} onClick={onBack} variant="quiet">Back</SettingsButton>
      </header>
      <div className="settings-form">
        {variables.map((variable) => (
          <SettingsField key={variable.name} label={`${variable.label ?? variable.name}${variable.optional ? " (optional)" : ""}`}>
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
          </SettingsField>
        ))}
      </div>
      <div className="settings-step-actions">
        <SettingsButton disabled={missingRequired} type="submit" variant="primary">Save</SettingsButton>
        {method.link ? <a href={method.link} rel="noopener noreferrer" target="_blank">Get a key</a> : null}
      </div>
    </form>
  );
}

function authMethodIcon(method: AgentAuthMethod) {
  if (method.kind === "env_var") return <KeyRound aria-hidden="true" size={18} strokeWidth={1.6} />;
  if (method.kind === "terminal") return <Terminal aria-hidden="true" size={18} strokeWidth={1.6} />;
  return <ExternalLink aria-hidden="true" size={18} strokeWidth={1.6} />;
}
