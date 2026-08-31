import type { McpServerDefinition } from "@openaide/app-server-client";
import type {
  McpServerSettingsRecord,
  SettingsProjectionAvailability,
} from "@openaide/app-shell-contracts";
import {
  AlertTriangle,
  ChevronRight,
  FolderGit2,
  Globe2,
  Plus,
} from "lucide-react";
import { useMemo, useRef, useState } from "react";

import type { McpServerSaveInput } from "../../intents/mcpSettingsIntents";
import type { ProjectOption } from "../../state/composerOptions";
import { McpServerEditor } from "./McpServerEditor";
import { McpToggle, McpTransportIcon } from "./McpSettingsControls";
import { SettingsCatalogSearch } from "./SettingsCatalogSearch";
import { EmptySettingsState, InlineFailure, SettingsSkeleton } from "./settingsPresentation";

type McpSettingsTabProps = {
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

export function McpSettingsTab(props: McpSettingsTabProps) {
  const [query, setQuery] = useState("");
  const [selected, setSelected] = useState<McpServerDefinition>();
  const [detailsLoading, setDetailsLoading] = useState(false);
  const [detailsError, setDetailsError] = useState<string>();
  const generation = useRef(0);
  const visible = useMemo(() => filterServers(props.servers ?? [], query), [props.servers, query]);

  const openServer = async (record: McpServerSettingsRecord) => {
    const current = ++generation.current;
    setDetailsLoading(true);
    setDetailsError(undefined);
    try {
      const server = await props.onLoadServer(record.id);
      if (current === generation.current) setSelected(server);
    } catch (error) {
      if (current === generation.current) {
        setDetailsError(error instanceof Error ? error.message : "Unable to load MCP server.");
      }
    } finally {
      if (current === generation.current) setDetailsLoading(false);
    }
  };
  const closeDetails = () => {
    generation.current += 1;
    setSelected(undefined);
    setDetailsError(undefined);
    setDetailsLoading(false);
  };

  if (selected) {
    return (
      <McpServerEditor
        initial={selected}
        onBack={closeDetails}
        onDelete={(server) => {
          props.onDeleteServer(server);
          closeDetails();
        }}
        onSave={(input) => {
          props.onSaveServer(input);
          closeDetails();
        }}
        onSetEnabled={props.onSetEnabled}
        projects={props.projects}
      />
    );
  }
  if (detailsLoading) return <SettingsSkeleton />;
  if (detailsError) return <InlineFailure message={detailsError} />;
  if (props.loading && !props.servers) return <SettingsSkeleton />;
  if (props.error && !props.servers) return <InlineFailure message={props.error} />;
  if (props.availability === "unavailable") {
    return <EmptySettingsState title="MCP settings unavailable" detail="The App Server could not read MCP configuration." />;
  }

  return (
    <section className="mcp-settings-page">
      <div className="mcp-library-intro">
        <p>Connect tools agents can use. Enable a server here; open it only when its connection needs attention.</p>
        <SettingsCatalogSearch
          label="Search MCP servers"
          onChange={setQuery}
          placeholder="Search"
          value={query}
        />
      </div>
      <div className="mcp-library-heading">
        <strong>Servers</strong>
        <button className="mcp-action primary" onClick={() => setSelected(newServer())} type="button">
          <Plus size={14} /><span>Add server</span>
        </button>
      </div>
      {props.error ? <InlineFailure message={props.error} muted /> : null}
      {!visible.length ? (
        <EmptySettingsState title="No MCP servers" detail={query ? "No servers match this search." : "Add a server to make tools available to agents."} />
      ) : (
        <div className="mcp-settings-groups">
          {groupServers(visible, props.projects).map((group) => (
            <McpServerGroup
              key={group.key}
              label={group.label}
              onOpen={(record) => void openServer(record)}
              onSetEnabled={props.onSetEnabled}
              records={group.records}
              scope={group.scope}
            />
          ))}
        </div>
      )}
    </section>
  );
}

function McpServerGroup({
  label,
  onOpen,
  onSetEnabled,
  records,
  scope,
}: {
  label: string;
  onOpen: (record: McpServerSettingsRecord) => void;
  onSetEnabled: (id: string, enabled: boolean) => void;
  records: McpServerSettingsRecord[];
  scope: "global" | "project";
}) {
  return (
    <section className="mcp-settings-group">
      <header className="mcp-settings-group-heading">
        {scope === "global" ? <Globe2 size={15} /> : <FolderGit2 size={15} />}
        <strong>{label}</strong>
      </header>
      <div className="mcp-settings-list">
        {records.map((record) => (
          <div className="mcp-settings-row" key={record.id}>
            <button aria-label={`Open ${record.label}`} className="mcp-settings-row-main" onClick={() => onOpen(record)} type="button">
              <McpTransportIcon transport={record.transport} />
              <span className="mcp-settings-row-copy">
                <strong>{record.label}</strong>
                <small>{record.description ?? record.id}</small>
              </span>
              {record.status === "invalid" ? <McpStatus /> : <span className="mcp-settings-transport">{record.transport.toUpperCase()}</span>}
              <ChevronRight size={14} />
            </button>
            <McpToggle
              checked={record.enabled}
              label={`${record.enabled ? "Disable" : "Enable"} ${record.label}`}
              onChange={(enabled) => onSetEnabled(record.id, enabled)}
            />
          </div>
        ))}
      </div>
    </section>
  );
}

function McpStatus() {
  return <span className="mcp-status invalid"><AlertTriangle size={12} />Needs attention</span>;
}

function newServer(): McpServerDefinition {
  return {
    id: `mcp.${crypto.randomUUID()}`,
    label: "",
    enabled: true,
    scope: { kind: "global" },
    configuration: { transport: "stdio", commandLine: "", command: "" },
  };
}

function filterServers(servers: McpServerSettingsRecord[], query: string) {
  const normalized = query.trim().toLowerCase();
  if (!normalized) return servers;
  return servers.filter((server) => (
    `${server.label} ${server.description ?? ""} ${server.transport} ${server.status}`
      .toLowerCase()
      .includes(normalized)
  ));
}

function groupServers(servers: McpServerSettingsRecord[], projects: ProjectOption[]) {
  const labels = new Map(projects.map((project) => [project.projectId, project.label]));
  const groups = new Map<string, {
    key: string;
    label: string;
    records: McpServerSettingsRecord[];
    scope: "global" | "project";
  }>();
  for (const record of servers) {
    const key = record.scope.kind === "global" ? "global" : `project:${record.scope.projectId}`;
    const label = record.scope.kind === "global"
      ? "Global"
      : labels.get(record.scope.projectId) ?? record.scope.projectId;
    const current = groups.get(key);
    if (current) current.records.push(record);
    else groups.set(key, { key, label, records: [record], scope: record.scope.kind });
  }
  return [...groups.values()];
}
