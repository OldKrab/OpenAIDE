import type { FrontendShell } from "../services/frontendShell";
import type { WebviewBootstrap } from "../state/surfaceTypes";

/**
 * The Project files page opened from a Project. The App Shell owns it as a route
 * (`projectFiles` in the bootstrap), so reload and Back keep it; Browse files is offered only by
 * shells that have a file viewer and route the page.
 */
export function projectFilesRoute(bootstrap: WebviewBootstrap, shell: FrontendShell | undefined) {
  const navigation = shell?.fileViewer === true ? shell.navigation : undefined;
  const routed = Boolean(navigation?.openProjectFiles && navigation.closeProjectFiles);
  const projectId = routed && bootstrap.surface === "task" && bootstrap.projectFiles && !bootstrap.taskId
    ? bootstrap.projectId
    : undefined;
  return {
    browse: routed
      ? (id: string) => {
        console.info(`project_files_page_opened project_id=${id}`);
        navigation?.openProjectFiles?.(id);
      }
      : undefined,
    page: projectId
      ? { projectId, onClose: () => navigation?.closeProjectFiles?.(projectId) }
      : undefined,
  };
}
