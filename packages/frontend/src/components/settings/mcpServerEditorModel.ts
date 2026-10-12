import type { McpServerConfiguration, McpServerDefinition } from "@openaide/app-server-client";
import { parseAgentCommandLine } from "../../intents/agentSettingsIntents";
import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";

export type ConfigField = { id: string; name: string; value: string; secret: boolean };

/** The form's working copy. Secret values are write-only, so a stored secret starts empty. */
export type McpEditorState = {
  server: McpServerDefinition;
  commandLine: string;
  fields: ConfigField[];
};

export function newMcpServer(): McpServerDefinition {
  return {
    id: `mcp.${crypto.randomUUID()}`,
    label: "",
    enabled: true,
    scope: { kind: "global" },
    configuration: { transport: "stdio", commandLine: "", command: "" },
  };
}

/** A definition made by `newMcpServer` that has not been saved yet. */
export function isNewMcpServer(server: McpServerDefinition) {
  return server.label === "" && server.id.startsWith("mcp.");
}

export function editorState(server: McpServerDefinition): McpEditorState {
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
      ...Object.entries(plain).map(([name, value]) => ({ id: crypto.randomUUID(), name, value, secret: false })),
      ...secret.map((name) => ({ id: crypto.randomUUID(), name, value: "", secret: true })),
    ],
  };
}

/** What Save would send, without the field ids that exist only for rendering. */
export function editorFingerprint(editor: McpEditorState, includeEnabled: boolean) {
  const { enabled, ...server } = editor.server;
  return JSON.stringify([
    includeEnabled ? enabled : null,
    { ...server, configuration: server.configuration.transport },
    editor.commandLine,
    editor.fields.map(({ name, value, secret }) => [name, value, secret]),
  ]);
}

/** Builds the save request, or throws the sentence that tells the user what to fix. */
export function saveInputFromEditor(editor: McpEditorState, initial: McpServerDefinition): McpServerSaveInput {
  const label = editor.server.label.trim();
  if (!label) throw new Error("Name is required.");
  const configuration = configurationFromEditor(editor);
  const transport = configuration.transport;
  validateNewSecretValues(editor.fields, initial.configuration, transport);
  return {
    server: { ...editor.server, label, configuration },
    previous: isNewMcpServer(initial) ? undefined : initial,
    secretValues: editor.fields.flatMap((field) => (
      field.secret && field.value
        ? [{ field: transport === "stdio" ? "env" as const : "header" as const, name: field.name, value: field.value }]
        : []
    )),
  };
}

function configurationFromEditor(editor: McpEditorState): McpServerConfiguration {
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
    return { transport: "stdio", commandLine: editor.commandLine, ...parsed, env: plain, secretEnv: secret };
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

/** Environment and headers do not carry across transports, so a switch starts them empty. */
export function changeTransport(
  editor: McpEditorState,
  transport: McpServerConfiguration["transport"],
): McpEditorState {
  if (transport === editor.server.configuration.transport) return editor;
  const configuration: McpServerConfiguration = transport === "stdio"
    ? { transport, commandLine: "", command: "" }
    : { transport, url: "" };
  return { server: { ...editor.server, configuration }, commandLine: "", fields: [] };
}

export function withServer(editor: McpEditorState, patch: Partial<McpServerDefinition>): McpEditorState {
  return { ...editor, server: { ...editor.server, ...patch } };
}

export function emptyConfigField(): ConfigField {
  return { id: crypto.randomUUID(), name: "", value: "", secret: false };
}

export function updateField(fields: ConfigField[], id: string, patch: Partial<ConfigField>) {
  return fields.map((field) => field.id === id ? { ...field, ...patch } : field);
}
