import type { McpServerDefinition } from "@openaide/app-server-client";
import type { McpServerSettingsRecord, SettingsProjectionAvailability } from "@openaide/app-shell-contracts";
import { Network, Plus } from "lucide-react";
import { useRef, useState } from "react";
import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";
import type { ProjectOption } from "../../state/composerOptions";
import { McpServerPage, mcpTransportIcon } from "./McpServerPage";
import { newMcpServer } from "./mcpServerEditorModel";
import { settingsInfo } from "./settingsInfo";
import { errorSentence, SettingsBlankState, SettingsButton, SettingsConfirm, SettingsInlineError, SettingsNotice, SettingsSwitch } from "./settingsKit";
import type { SettingsGroup, SettingsSection } from "./settingsModel";

export type McpSectionProps = {
  available: boolean;
  availability?: SettingsProjectionAvailability;
  error?: string;
  loading?: boolean;
  onDeleteServer: (server: McpServerDefinition) => void;
  onLoadServer: (id: string) => Promise<McpServerDefinition>;
  onSaveServer: (input: McpServerSaveInput) => void;
  onSetEnabled: (id: string, enabled: boolean) => void;
  projects: ProjectOption[];
  servers?: McpServerSettingsRecord[];
};

/** Tool servers grouped by where they apply. The list is App Server state; a page loads its full definition on open. */
export function useMcpSection(props: McpSectionProps): SettingsSection | undefined {
  const [selected, setSelected] = useState<McpServerDefinition>();
  const [opening, setOpening] = useState(false);
  const [openError, setOpenError] = useState<string>();
  const [confirm, setConfirm] = useState<"delete" | "discard">();
  const generation = useRef(0);
  const dirty = useRef(false);
  if (!props.available) return undefined;

  const openServer = async (record: McpServerSettingsRecord) => {
    const current = ++generation.current;
    setOpening(true);
    setOpenError(undefined);
    try {
      const server = await props.onLoadServer(record.id);
      if (current === generation.current) setSelected(server);
    } catch (error) {
      if (current === generation.current) setOpenError(errorSentence(error, "Unable to load MCP server."));
    } finally {
      if (current === generation.current) setOpening(false);
    }
  };
  const leaveDetail = () => {
    generation.current += 1;
    dirty.current = false;
    setConfirm(undefined);
    setSelected(undefined);
    setOpenError(undefined);
    setOpening(false);
  };
  const closeDetail = () => {
    if (dirty.current) {
      setConfirm("discard");
      return false;
    }
    leaveDetail();
    return true;
  };

  const servers = props.servers;
  const unavailable = props.availability === "unavailable";
  const groups: SettingsGroup[] = servers && !unavailable
    ? groupServers(servers, props.projects).map((group) => ({
      id: group.key,
      title: group.label,
      rows: group.records.map((record) => ({
        id: record.id,
        label: record.label,
        description: record.description ? `${transportLabel(record.transport)} · ${record.description}` : transportLabel(record.transport),
        keywords: `mcp server tool ${record.transport} ${record.id}`,
        leading: mcpTransportIcon(record.transport),
        status: record.status === "invalid" && record.enabled ? { tone: "warn" as const, text: "Needs attention" } : undefined,
        control: <SettingsSwitch checked={record.enabled} label={`${record.label} enabled`} onChange={(enabled) => props.onSetEnabled(record.id, enabled)} />,
        onOpen: () => void openServer(record),
      })),
    }))
    : [];
  if (servers && !unavailable && !groups.length) groups.push({ id: "global", title: "Global", empty: "None.", rows: [] });

  return {
    id: "mcp",
    label: "MCP servers",
    icon: Network,
    info: settingsInfo.mcp,
    action: servers && !unavailable ? (
      <SettingsButton icon={<Plus aria-hidden="true" size={14} />} onClick={() => setSelected(newMcpServer())} variant="primary">Add server</SettingsButton>
    ) : undefined,
    notice: (
      <>
        {unavailable ? <SettingsBlankState detail="The App Server could not read MCP configuration." title="MCP settings unavailable" />
          : !servers && props.loading ? <SettingsBlankState title="Loading MCP servers…" /> : null}
        {props.error && !unavailable ? <SettingsInlineError>{props.error}</SettingsInlineError> : null}
        {openError ? <SettingsInlineError>{openError}</SettingsInlineError> : null}
        {opening ? <SettingsNotice>Opening server…</SettingsNotice> : null}
      </>
    ),
    groups,
    closeDetail: selected ? closeDetail : undefined,
    detail: selected ? (
      <McpServerPage
        initial={selected}
        onBack={closeDetail}
        onDelete={() => setConfirm("delete")}
        onDirtyChange={(next) => { dirty.current = next; }}
        onSave={(input) => {
          props.onSaveServer(input);
          leaveDetail();
        }}
        onSetEnabled={props.onSetEnabled}
        projects={props.projects}
      />
    ) : undefined,
    overlay: selected && confirm ? (
      confirm === "delete" ? (
        <SettingsConfirm
          confirmLabel="Delete server"
          detail="Agents lose the tools it provides. This cannot be undone."
          label="Delete MCP server confirmation"
          onClose={() => setConfirm(undefined)}
          onConfirm={() => {
            props.onDeleteServer(selected);
            leaveDetail();
          }}
          title={`Delete ${selected.label || "this server"}?`}
        />
      ) : (
        <SettingsConfirm
          confirmLabel="Discard"
          detail="Your edits to this server are not saved."
          label="Discard changes confirmation"
          onClose={() => setConfirm(undefined)}
          onConfirm={leaveDetail}
          title="Discard changes?"
        />
      )
    ) : undefined,
  };
}

function transportLabel(transport: McpServerSettingsRecord["transport"]) {
  return transport === "stdio" ? "stdio" : transport.toUpperCase();
}

function groupServers(servers: McpServerSettingsRecord[], projects: ProjectOption[]) {
  const labels = new Map(projects.map((project) => [project.projectId, project.label]));
  const groups = new Map<string, { key: string; label: string; records: McpServerSettingsRecord[] }>();
  for (const record of servers) {
    const key = record.scope.kind === "global" ? "global" : `project:${record.scope.projectId}`;
    const label = record.scope.kind === "global"
      ? "Global"
      : labels.get(record.scope.projectId) ?? record.scope.projectId;
    const current = groups.get(key);
    if (current) current.records.push(record);
    else groups.set(key, { key, label, records: [record] });
  }
  return [...groups.values()];
}
