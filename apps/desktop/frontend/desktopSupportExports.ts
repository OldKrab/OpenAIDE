import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

/** Keeps native path selection and export persistence outside the shared Frontend. */
export function createDesktopSupportExports(invoke: Invoke): NonNullable<FrontendShell["supportExports"]> {
  return {
    async save({ fileHandleId, label }) {
      const saved = await invoke("desktop_save_support_export", { fileHandleId, label });
      if (typeof saved !== "boolean") throw new Error("Desktop returned an invalid support export result.");
      return saved ? "saved" : "cancelled";
    },
  };
}
