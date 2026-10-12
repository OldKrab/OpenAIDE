import type { McpServerConfiguration, McpServerDefinition, ProjectId } from "@openaide/app-server-client";
import { Globe2, KeyRound, Plus, Server, TerminalSquare } from "lucide-react";
import { useEffect, useState } from "react";
import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";
import type { ProjectOption } from "../../state/composerOptions";
import {
  changeTransport,
  editorFingerprint,
  editorState,
  emptyConfigField,
  isNewMcpServer,
  saveInputFromEditor,
  updateField,
  withServer,
  type ConfigField,
} from "./mcpServerEditorModel";
import {
  SettingsButton,
  SettingsCrumb,
  SettingsField,
  SettingsIdentity,
  SettingsInlineError,
  SettingsSegmented,
  SettingsSelect,
  SettingsSwitch,
} from "./settingsKit";

type Transport = McpServerConfiguration["transport"];

export function mcpTransportIcon(transport: Transport, size = 18) {
  const Icon = transport === "stdio" ? TerminalSquare : transport === "http" ? Globe2 : Server;
  return <Icon aria-hidden="true" size={size} strokeWidth={size > 20 ? 1.5 : 1.6} />;
}

/** One MCP server, edited in place. Nothing but the availability switch is sent before Save. */
export function McpServerPage({
  initial,
  onBack,
  onDelete,
  onDirtyChange,
  onSave,
  onSetEnabled,
  projects,
}: {
  initial: McpServerDefinition;
  onBack: () => void;
  onDelete: () => void;
  onDirtyChange: (dirty: boolean) => void;
  onSave: (input: McpServerSaveInput) => void;
  onSetEnabled: (id: string, enabled: boolean) => void;
  projects: ProjectOption[];
}) {
  const creating = isNewMcpServer(initial);
  const [editor, setEditor] = useState(() => editorState(initial));
  const [saved, setSaved] = useState(() => editorFingerprint(editor, creating));
  const [formError, setFormError] = useState<string>();
  useEffect(() => {
    const next = editorState(initial);
    setEditor(next);
    setSaved(editorFingerprint(next, creating));
  }, [initial]);
  const dirty = editorFingerprint(editor, creating) !== saved;
  useEffect(() => { onDirtyChange(dirty); }, [dirty]);

  const transport = editor.server.configuration.transport;
  const scopeValue = editor.server.scope.kind === "global" ? "global" : editor.server.scope.projectId;
  const title = editor.server.label || "New MCP server";
  const save = () => {
    try {
      onSave(saveInputFromEditor(editor, initial));
    } catch (error) {
      setFormError(error instanceof Error ? error.message : "MCP configuration is invalid.");
    }
  };

  return (
    <section aria-label="MCP server details" className="settings-detail">
      <SettingsCrumb label="MCP servers" onBack={onBack} />
      <SettingsIdentity
        end={(
          <SettingsSwitch
            checked={editor.server.enabled}
            label={`${title} enabled`}
            onChange={(enabled) => {
              setEditor(withServer(editor, { enabled }));
              if (!creating) onSetEnabled(initial.id, enabled);
            }}
          />
        )}
        icon={mcpTransportIcon(transport, 24)}
        subtitle={creating ? "Not saved yet" : editor.server.description}
        title={title}
      />
      <section className="settings-group">
        <header><h2>Connection</h2></header>
        <div className="settings-card settings-form">
          <SettingsField label="Name">
            <input
              aria-label="Server name"
              onChange={(event) => setEditor(withServer(editor, { label: event.currentTarget.value }))}
              value={editor.server.label}
            />
          </SettingsField>
          <SettingsField label="Description">
            <input
              aria-label="Server description"
              onChange={(event) => setEditor(withServer(editor, { description: event.currentTarget.value || undefined }))}
              value={editor.server.description ?? ""}
            />
          </SettingsField>
          <div className="settings-form-pair">
            <div className="settings-field">
              <span>Available in</span>
              <SettingsSelect
                label="Available in"
                onChange={(value) => setEditor(withServer(editor, {
                  scope: value === "global" ? { kind: "global" } : { kind: "project", projectId: value as ProjectId },
                }))}
                options={[
                  { value: "global", label: "All projects" },
                  ...projects.map((project) => ({ value: project.projectId, label: project.label })),
                ]}
                value={scopeValue}
              />
            </div>
            <div className="settings-field">
              <span>Transport</span>
              <SettingsSegmented<Transport>
                label="Transport"
                onChange={(next) => setEditor(changeTransport(editor, next))}
                options={[
                  { value: "stdio", label: "stdio" },
                  { value: "http", label: "HTTP" },
                  { value: "sse", label: "SSE" },
                ]}
                value={transport}
              />
              {transport === "sse" ? <small>SSE is deprecated. Use HTTP when the server supports it.</small> : null}
            </div>
          </div>
          <SettingsField label={transport === "stdio" ? "Command" : "URL"}>
            <input
              aria-label={transport === "stdio" ? "Command" : "URL"}
              className="settings-mono"
              onChange={(event) => setEditor({ ...editor, commandLine: event.currentTarget.value })}
              placeholder={transport === "stdio" ? "/absolute/path/to/server --arg" : "https://example.com/mcp"}
              spellCheck={false}
              value={editor.commandLine}
            />
          </SettingsField>
        </div>
      </section>
      <ConfigFields fields={editor.fields} onChange={(fields) => setEditor({ ...editor, fields })} transport={transport} />
      {formError ? <SettingsInlineError>{formError}</SettingsInlineError> : null}
      <footer className="settings-detail-footer">
        {creating ? <span /> : <SettingsButton danger onClick={onDelete} variant="quiet">Delete server…</SettingsButton>}
        {dirty || creating ? (
          <span className="settings-save">
            <SettingsButton onClick={onBack} variant="quiet">Cancel</SettingsButton>
            <SettingsButton onClick={save} variant="primary">{creating ? "Add server" : "Save"}</SettingsButton>
          </span>
        ) : null}
      </footer>
    </section>
  );
}

function ConfigFields({
  fields,
  onChange,
  transport,
}: {
  fields: ConfigField[];
  onChange: (fields: ConfigField[]) => void;
  transport: Transport;
}) {
  const noun = transport === "stdio" ? "Environment" : "Headers";
  return (
    <section className="settings-group">
      <header>
        <h2>{noun}</h2>
        <span className="settings-group-actions">
          <SettingsButton icon={<Plus aria-hidden="true" size={14} />} onClick={() => onChange([...fields, emptyConfigField()])}>
            {transport === "stdio" ? "Add variable" : "Add header"}
          </SettingsButton>
        </span>
      </header>
      {fields.length ? (
        <ul className="settings-card settings-env">
          {fields.map((field) => (
            <li key={field.id}>
              <input
                aria-label={`${noun} name`}
                className="settings-mono"
                onChange={(event) => onChange(updateField(fields, field.id, { name: event.currentTarget.value }))}
                placeholder="Name"
                value={field.name}
              />
              <input
                aria-label={`${noun} value`}
                className="settings-mono"
                onChange={(event) => onChange(updateField(fields, field.id, { value: event.currentTarget.value }))}
                placeholder={field.secret ? "Stored secret" : "Value"}
                type={field.secret ? "password" : "text"}
                value={field.value}
              />
              <span className="settings-env-secret">
                <KeyRound aria-hidden="true" size={13} />Secret
                <SettingsSwitch
                  checked={field.secret}
                  label={`${field.name || "Field"} is secret`}
                  onChange={(secret) => onChange(updateField(fields, field.id, { secret, value: "" }))}
                />
              </span>
              <SettingsButton ariaLabel={`Remove ${field.name || "field"}`} onClick={() => onChange(fields.filter((item) => item.id !== field.id))} variant="quiet">
                Remove
              </SettingsButton>
            </li>
          ))}
        </ul>
      ) : <p className="settings-empty">None.</p>}
    </section>
  );
}
