import type {
  McpServerConfiguration,
  McpServerDefinition,
  ProjectId,
} from "@openaide/app-server-client";
import {
  ArrowLeft,
  Check,
  ChevronDown,
  FileKey2,
  Globe2,
  Network,
  Plus,
  Server,
  Terminal,
  Trash2,
  X,
} from "lucide-react";
import { useEffect, useRef, useState, type ReactNode } from "react";

import { parseAgentCommandLine } from "../../intents/agentSettingsIntents";
import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";
import type { ProjectOption } from "../../state/composerOptions";
import { McpToggle, McpTransportIcon } from "./McpSettingsControls";
import { InlineFailure } from "./settingsPresentation";

type ConfigField = { id: string; name: string; value: string; secret: boolean };
type EditorState = {
  server: McpServerDefinition;
  commandLine: string;
  fields: ConfigField[];
};

export function McpServerEditor({
  initial,
  onBack,
  onDelete,
  onSave,
  onSetEnabled,
  projects,
}: {
  initial: McpServerDefinition;
  onBack: () => void;
  onDelete: (server: McpServerDefinition) => void;
  onSave: (input: McpServerSaveInput) => void;
  onSetEnabled: (id: string, enabled: boolean) => void;
  projects: ProjectOption[];
}) {
  const creating = initial.label === "" && initial.id.startsWith("mcp.");
  const [editor, setEditor] = useState(() => editorState(initial));
  const [confirmDelete, setConfirmDelete] = useState(false);
  const [formError, setFormError] = useState<string>();
  useEffect(() => setEditor(editorState(initial)), [initial]);
  const transport = editor.server.configuration.transport;
  const scopeValue = editor.server.scope.kind === "global" ? "global" : editor.server.scope.projectId;
  const scopeOptions = [
    { value: "global", label: "Global" },
    ...projects.map((project) => ({ value: project.projectId, label: project.label })),
  ];

  const save = () => {
    try {
      const label = editor.server.label.trim();
      if (!label) throw new Error("Name is required.");
      const configuration = configurationFromEditor(editor);
      validateNewSecretValues(editor.fields, initial.configuration, configuration.transport);
      onSave({
        server: { ...editor.server, label, configuration },
        previous: creating ? undefined : initial,
        secretValues: editor.fields.flatMap((field) => (
          field.secret && field.value
            ? [{
                field: transport === "stdio" ? "env" as const : "header" as const,
                name: field.name,
                value: field.value,
              }]
            : []
        )),
      });
    } catch (error) {
      setFormError(error instanceof Error ? error.message : "MCP configuration is invalid.");
    }
  };

  return (
    <section className="mcp-editor">
      <button className="mcp-back" onClick={onBack} type="button">
        <ArrowLeft size={14} /><span>Back to MCP Servers</span>
      </button>
      <header className="mcp-editor-header">
        <McpTransportIcon large transport={transport} />
        <span className="mcp-editor-identity">
          <h2>{editor.server.label || "New MCP server"}</h2>
          <p>{editor.server.description || "Configure a tool server."}</p>
          <small>{formError ? "Connection needs attention" : (creating ? "Not saved yet" : "Configured")}</small>
        </span>
        <span className="mcp-editor-availability">
          <McpToggle
            checked={editor.server.enabled}
            label="MCP server enabled"
            onChange={(enabled) => {
              setEditor(withServer(editor, { enabled }));
              if (!creating) onSetEnabled(initial.id, enabled);
            }}
          />
          <small>{editor.server.enabled ? "Available" : "Unavailable"}</small>
        </span>
      </header>
      {formError ? <InlineFailure message={formError} /> : null}
      <section className="mcp-editor-section">
        <h3>Identity</h3>
        <div className="mcp-editor-panel">
          <EditorRow icon={<Server size={16} />} label="Name">
            <input
              aria-label="Server name"
              onChange={(event) => setEditor(withServer(editor, { label: event.currentTarget.value }))}
              value={editor.server.label}
            />
          </EditorRow>
          <EditorRow icon={<Network size={16} />} label="Description">
            <input
              aria-label="Server description"
              onChange={(event) => setEditor(withServer(editor, {
                description: event.currentTarget.value || undefined,
              }))}
              value={editor.server.description ?? ""}
            />
          </EditorRow>
        </div>
      </section>
      <section className="mcp-editor-section">
        <h3>Connection</h3>
        <div className="mcp-editor-panel">
          <EditorRow icon={<Globe2 size={16} />} label="Scope">
            <PopupSelect
              label="Scope"
              onChange={(value) => setEditor(withServer(editor, {
                scope: value === "global"
                  ? { kind: "global" }
                  : { kind: "project", projectId: value as ProjectId },
              }))}
              options={scopeOptions}
              value={scopeValue}
            />
          </EditorRow>
          <EditorRow icon={<Terminal size={16} />} label="Transport">
            <PopupSelect
              label="Transport"
              onChange={(value) => setEditor(changeTransport(
                editor,
                value as McpServerConfiguration["transport"],
              ))}
              options={[
                { value: "stdio", label: "stdio" },
                { value: "http", label: "HTTP" },
                { value: "sse", label: "SSE (deprecated)" },
              ]}
              value={transport}
            />
          </EditorRow>
          <EditorRow icon={<Network size={16} />} label={transport === "stdio" ? "Command" : "URL"}>
            <input
              aria-label={transport === "stdio" ? "Command" : "URL"}
              className="mono"
              onChange={(event) => setEditor({ ...editor, commandLine: event.currentTarget.value })}
              placeholder={transport === "stdio"
                ? "/absolute/path/to/server --arg"
                : "https://example.com/mcp"}
              value={editor.commandLine}
            />
          </EditorRow>
        </div>
      </section>
      <ConfigFields
        fields={editor.fields}
        onChange={(fields) => setEditor({ ...editor, fields })}
        transport={transport}
      />
      <div className="mcp-editor-actions">
        {!creating ? (
          confirmDelete ? (
            <span className="mcp-delete-confirm">
              <span>Delete this server?</span>
              <button className="mcp-action danger" onClick={() => onDelete(initial)} type="button">
                Delete
              </button>
              <button className="mcp-action" onClick={() => setConfirmDelete(false)} type="button">
                Cancel
              </button>
            </span>
          ) : (
            <button
              className="mcp-action danger quiet"
              onClick={() => setConfirmDelete(true)}
              type="button"
            >
              <Trash2 size={14} /><span>Delete</span>
            </button>
          )
        ) : <span />}
        <span>
          <button className="mcp-action" onClick={onBack} type="button">
            <X size={14} /><span>Cancel</span>
          </button>
          <button className="mcp-action primary" onClick={save} type="button">
            <Check size={14} /><span>Save</span>
          </button>
        </span>
      </div>
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
  transport: McpServerConfiguration["transport"];
}) {
  const noun = transport === "stdio" ? "Environment" : "Headers";
  return (
    <section className="mcp-editor-section">
      <div className="mcp-editor-section-heading">
        <h3>{noun}</h3>
        <button
          className="mcp-action"
          onClick={() => onChange([...fields, emptyConfigField()])}
          type="button"
        >
          <Plus size={13} /><span>Add field</span>
        </button>
      </div>
      {!fields.length ? (
        <div className="mcp-editor-panel">
          <div className="mcp-config-summary">
            <span className="mcp-editor-row-icon"><FileKey2 size={16} /></span>
            <span><strong>Variables</strong><small>No {noun.toLowerCase()} configured.</small></span>
          </div>
        </div>
      ) : (
        <div className="mcp-fields">
          {fields.map((field) => (
            <div className="mcp-field-row" key={field.id}>
              <input
                aria-label={`${noun} name`}
                onChange={(event) => onChange(updateField(
                  fields,
                  field.id,
                  { name: event.currentTarget.value },
                ))}
                placeholder="Name"
                value={field.name}
              />
              <input
                aria-label={`${noun} value`}
                onChange={(event) => onChange(updateField(
                  fields,
                  field.id,
                  { value: event.currentTarget.value },
                ))}
                placeholder={field.secret ? "Stored securely" : "Value"}
                type={field.secret ? "password" : "text"}
                value={field.value}
              />
              <label className="mcp-secret-check">
                <input
                  checked={field.secret}
                  onChange={(event) => onChange(updateField(fields, field.id, {
                    secret: event.currentTarget.checked,
                    value: "",
                  }))}
                  type="checkbox"
                />
                Secret
              </label>
              <button
                aria-label={`Remove ${field.name || "field"}`}
                className="mcp-icon-action"
                onClick={() => onChange(fields.filter((item) => item.id !== field.id))}
                type="button"
              >
                <X size={14} />
              </button>
            </div>
          ))}
        </div>
      )}
    </section>
  );
}

function EditorRow({
  children,
  icon,
  label,
}: {
  children: ReactNode;
  icon: ReactNode;
  label: string;
}) {
  return (
    <div className="mcp-editor-row">
      <span className="mcp-editor-row-icon">{icon}</span>
      <strong>{label}</strong>
      <span className="mcp-editor-row-action">{children}</span>
    </div>
  );
}

function PopupSelect({
  label,
  onChange,
  options,
  value,
}: {
  label: string;
  onChange: (value: string) => void;
  options: Array<{ value: string; label: string }>;
  value: string;
}) {
  const [open, setOpen] = useState(false);
  const ref = useRef<HTMLDivElement>(null);
  useEffect(() => {
    if (!open) return;
    const close = (event: PointerEvent) => {
      if (!ref.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", close);
    return () => document.removeEventListener("pointerdown", close);
  }, [open]);
  const selected = options.find((option) => option.value === value)?.label ?? value;
  return (
    <div className="mcp-popup" ref={ref}>
      <button
        aria-expanded={open}
        aria-haspopup="listbox"
        onClick={() => setOpen(!open)}
        type="button"
      >
        <span>{selected}</span>
        <ChevronDown size={14} />
      </button>
      {open ? (
        <div aria-label={label} className="mcp-popup-menu" role="listbox">
          {options.map((option) => (
            <button
              key={option.value}
              onClick={() => {
                onChange(option.value);
                setOpen(false);
              }}
              role="option"
              type="button"
            >
              <span>{option.label}</span>
              {option.value === value ? <Check size={13} /> : null}
            </button>
          ))}
        </div>
      ) : null}
    </div>
  );
}

function editorState(server: McpServerDefinition): EditorState {
  const configuration = server.configuration;
  const plain = configuration.transport === "stdio"
    ? configuration.env ?? {}
    : configuration.headers ?? {};
  const secret = configuration.transport === "stdio"
    ? configuration.secretEnv ?? []
    : configuration.secretHeaders ?? [];
  return {
    server,
    commandLine: configuration.transport === "stdio" ? configuration.commandLine : configuration.url,
    fields: [
      ...Object.entries(plain).map(([name, value]) => ({
        id: crypto.randomUUID(),
        name,
        value,
        secret: false,
      })),
      ...secret.map((name) => ({
        id: crypto.randomUUID(),
        name,
        value: "",
        secret: true,
      })),
    ],
  };
}

function configurationFromEditor(editor: EditorState): McpServerConfiguration {
  const rows = editor.fields.filter((field) => field.name.trim());
  const names = new Set<string>();
  for (const field of rows) {
    const name = field.name.trim();
    if (names.has(name)) throw new Error(`Field ${name} is configured more than once.`);
    names.add(name);
  }
  const plain = Object.fromEntries(
    rows.filter((field) => !field.secret).map((field) => [field.name.trim(), field.value]),
  );
  const secret = rows.filter((field) => field.secret).map((field) => field.name.trim());
  if (editor.server.configuration.transport === "stdio") {
    const parsed = parseAgentCommandLine(editor.commandLine);
    if (!isAbsoluteCommand(parsed.command)) {
      throw new Error("Command must use an absolute path.");
    }
    return {
      transport: "stdio",
      commandLine: editor.commandLine,
      ...parsed,
      env: plain,
      secretEnv: secret,
    };
  }
  if (!/^https?:\/\//i.test(editor.commandLine.trim())) {
    throw new Error("URL must use HTTP or HTTPS.");
  }
  return {
    transport: editor.server.configuration.transport,
    url: editor.commandLine.trim(),
    headers: plain,
    secretHeaders: secret,
  };
}

function validateNewSecretValues(
  fields: ConfigField[],
  previous: McpServerConfiguration,
  nextTransport: McpServerConfiguration["transport"],
) {
  const previousKind = previous.transport === "stdio" ? "env" : "header";
  const previousNames = previous.transport === "stdio"
    ? previous.secretEnv ?? []
    : previous.secretHeaders ?? [];
  const previousKeys = new Set(previousNames.map((name) => `${previousKind}:${name}`));
  const nextKind = nextTransport === "stdio" ? "env" : "header";
  for (const field of fields) {
    const name = field.name.trim();
    if (field.secret && name && !field.value && !previousKeys.has(`${nextKind}:${name}`)) {
      throw new Error(`Enter a value for the new secret ${name}.`);
    }
  }
}

function isAbsoluteCommand(command: string) {
  return command.startsWith("/")
    || command.startsWith("\\\\")
    || /^[A-Za-z]:[\\/]/.test(command);
}

function changeTransport(
  editor: EditorState,
  transport: McpServerConfiguration["transport"],
): EditorState {
  if (transport === editor.server.configuration.transport) return editor;
  const configuration: McpServerConfiguration = transport === "stdio"
    ? { transport, commandLine: "", command: "" }
    : { transport, url: "" };
  return { server: { ...editor.server, configuration }, commandLine: "", fields: [] };
}

function withServer(editor: EditorState, patch: Partial<McpServerDefinition>) {
  return { ...editor, server: { ...editor.server, ...patch } };
}

function emptyConfigField(): ConfigField {
  return { id: crypto.randomUUID(), name: "", value: "", secret: false };
}

function updateField(fields: ConfigField[], id: string, patch: Partial<ConfigField>) {
  return fields.map((field) => field.id === id ? { ...field, ...patch } : field);
}
