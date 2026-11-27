import { Archive } from "lucide-react";
import type { AgentListedSession } from "@openaide/app-shell-contracts";
import type {
  TaskNavigationSearchHit,
  TaskNavigationSearchResult,
} from "@openaide/app-server-client";
import { mapProtocolNavigationRow } from "../state/appServerProtocolMapping";
import { chatReveal } from "../state/chatReveal";
import { AgentIcon, type AgentIconLookup } from "./AgentIcon";
import { relativeTime } from "./taskSurfaceHelpers";
import { useNavigationSearch, type RunNavigationSearch } from "./useNavigationSearch";

type SidebarSearchResultsProps = {
  activeTaskId?: string;
  agentIcons?: AgentIconLookup;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
  query: string;
  search: RunNavigationSearch;
};

/**
 * The sidebar list while the search field has text (ADR-0086, "Search"). It shows App
 * Server's one-shot answer, not the Navigation view, which stays subscribed beneath it.
 */
export function SidebarSearchResults({
  activeTaskId,
  agentIcons,
  onOpenNativeSession,
  onOpenTask,
  query,
  search,
}: SidebarSearchResultsProps) {
  const { state, retry } = useNavigationSearch(query, search);
  const shown: TaskNavigationSearchResult | undefined = state.status === "ready"
    ? state.result
    : state.status === "loading" ? state.previous : undefined;
  const hits = shown?.hits ?? [];
  return (
    <div className="task-list-shell">
      <div aria-busy={state.status === "loading"} aria-label="Search results" className="task-list search-results" role="list">
        {hits.map((hit) => (
          <SearchHitRow
            activeTaskId={activeTaskId}
            agentIcons={agentIcons}
            hit={hit}
            key={hitKey(hit)}
            onOpenNativeSession={onOpenNativeSession}
            onOpenTask={onOpenTask}
          />
        ))}
        {state.status === "error" ? (
          <p className="empty-list search-status" role="alert">
            Search failed.{" "}
            <button className="search-retry" onClick={retry} type="button">Try again</button>
          </p>
        ) : null}
        {state.status === "loading" && hits.length === 0 ? (
          <p aria-live="polite" className="empty-list search-status">Searching</p>
        ) : null}
        {state.status === "ready" && hits.length === 0 ? (
          <p aria-live="polite" className="empty-list search-status">No results</p>
        ) : null}
        {shown?.truncated && hits.length > 0 ? (
          <p className="search-status search-truncated">Showing the best matches. Refine the search to narrow them.</p>
        ) : null}
      </div>
    </div>
  );
}

function SearchHitRow({
  activeTaskId,
  agentIcons,
  hit,
  onOpenNativeSession,
  onOpenTask,
}: {
  activeTaskId?: string;
  agentIcons?: AgentIconLookup;
  hit: TaskNavigationSearchHit;
  onOpenNativeSession: (session: AgentListedSession) => void;
  onOpenTask: (taskId: string) => void;
}) {
  // The same row mapping the Navigation view uses, so a Native Session opens the same way.
  const { row } = mapProtocolNavigationRow(hit.row);
  const agentId = row.kind === "task" ? row.task.agent_id : row.session.agent_id ?? "";
  const rawTitle = row.kind === "task" ? row.task.title : row.session.title;
  const title = rawTitle?.trim() ? rawTitle : "Untitled task";
  // Highlights index the title App Server matched; the placeholder has none.
  const titleRanges = rawTitle?.trim() ? hit.titleHighlights ?? [] : [];
  const activity = row.kind === "task" ? row.task.last_activity : row.session.last_activity;
  const age = activity ? relativeTime(activity) : "";
  const selected = row.kind === "task" && row.task.task_id === activeTaskId;
  const open = () => {
    if (row.kind === "task") {
      if (hit.messageId) chatReveal.request(row.task.task_id, hit.messageId);
      onOpenTask(row.task.task_id);
      return;
    }
    onOpenNativeSession(row.session);
  };
  return (
    <div role="listitem">
      <button
        aria-current={selected ? "true" : undefined}
        className={`search-hit ${selected ? "selected" : ""}`}
        onClick={open}
        type="button"
      >
        <span aria-hidden="true" className="task-agent-icon">
          <AgentIcon agentId={agentId} agentName={agentId} icon={agentIcons?.[agentId]} size={12} />
        </span>
        <span className="search-hit-body">
          <span className="search-hit-title">
            <span className="task-title"><Highlighted ranges={titleRanges} text={title} /></span>
            {hit.section === "archive" ? (
              <span aria-label="Archived" className="search-hit-section" role="img" title="In the Archive">
                <Archive aria-hidden="true" size={11} />
              </span>
            ) : null}
            {age ? <span className="task-meta-age">{age}</span> : null}
          </span>
          {hit.snippet ? (
            <span className="search-hit-snippet">
              <Highlighted ranges={hit.snippet.highlights} text={hit.snippet.text} />
            </span>
          ) : null}
        </span>
      </button>
    </div>
  );
}

/** Marks `[start, end)` ranges counted in UTF-16 code units, which `String.slice` also uses. */
export function Highlighted({ ranges, text }: { ranges: ReadonlyArray<readonly [number, number]>; text: string }) {
  const parts: Array<{ marked: boolean; text: string }> = [];
  let cursor = 0;
  for (const [start, end] of ranges) {
    if (start < cursor || end <= start || end > text.length) continue;
    if (start > cursor) parts.push({ marked: false, text: text.slice(cursor, start) });
    parts.push({ marked: true, text: text.slice(start, end) });
    cursor = end;
  }
  if (cursor < text.length) parts.push({ marked: false, text: text.slice(cursor) });
  return (
    <>
      {parts.map((part, index) => part.marked
        ? <mark key={index}>{part.text}</mark>
        : <span key={index}>{part.text}</span>)}
    </>
  );
}

function hitKey(hit: TaskNavigationSearchHit) {
  const { entry } = hit.row;
  return entry.kind === "task"
    ? `task:${entry.task.taskId}`
    : `session:${entry.session.reference.agentId}:${entry.session.reference.sessionId}`;
}
