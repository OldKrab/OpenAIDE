import { getCurrentWindow } from "@tauri-apps/api/window";

/** Startup chrome belongs to the shell and must not wait for App Server readiness.
 * React replaces these nodes on handoff; their listeners then leave with them.
 */
export function initializeStartupWindow() {
  const header = document.querySelector<HTMLElement>(".desktop-startup-title-bar");
  if (!header) return;
  const nativeWindow = getCurrentWindow();
  header.onmousedown = (event) => {
    if (event.button !== 0 || event.detail !== 1 || (event.target as Element).closest("button")) return;
    event.preventDefault();
    void windowOperation("drag", () => nativeWindow.startDragging());
  };
  header.ondblclick = (event) => {
    if (event.button !== 0 || (event.target as Element).closest("button")) return;
    event.preventDefault();
    void windowOperation("maximize", () => nativeWindow.toggleMaximize());
  };
  const actions = {
    minimize: () => nativeWindow.minimize(),
    maximize: () => nativeWindow.toggleMaximize(),
    close: () => nativeWindow.close(),
  };
  for (const [action, run] of Object.entries(actions)) {
    const button = header.querySelector<HTMLButtonElement>(`[data-window-action="${action}"]`);
    if (button) button.onclick = () => { void windowOperation(action, run); };
  }
  // The native query is independent of runtime preparation, including WSL setup.
  // Decorated windows retain their native controls instead of getting duplicates.
  void windowOperation("initialize", async () => {
    const decorated = await nativeWindow.isDecorated();
    header.hidden = decorated;
  });
}

let nextOperationId = 0;
async function windowOperation(action: string, run: () => Promise<void>) {
  const operationId = ++nextOperationId;
  const startedAt = performance.now();
  console.info(`desktop_startup_window_started operation_id=${operationId} action=${action} attempt=1`);
  try {
    await run();
    console.info(`desktop_startup_window_completed operation_id=${operationId} action=${action} outcome=success duration_ms=${Math.round(performance.now() - startedAt)} attempt=1`);
  } catch {
    console.warn(`desktop_startup_window_completed operation_id=${operationId} action=${action} outcome=failure error_kind=native_window duration_ms=${Math.round(performance.now() - startedAt)} attempt=1`);
  }
}
