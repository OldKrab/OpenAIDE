import { homedir } from "node:os";
import * as path from "node:path";

export const SUPPORT_EXPORT_DIRECTORY_KEY = "openaide.supportExport.lastDirectory";

export type SupportExportState = {
  get<T>(key: string, defaultValue: T): T;
  update(key: string, value: string): PromiseLike<void> | void;
};

export function supportExportDefaultPath(state: SupportExportState | undefined, label: string) {
  const directory = state?.get(SUPPORT_EXPORT_DIRECTORY_KEY, homedir()) ?? homedir();
  return path.join(directory, label);
}

export async function rememberSupportExportDirectory(state: SupportExportState | undefined, filePath: string) {
  await state?.update(SUPPORT_EXPORT_DIRECTORY_KEY, path.dirname(filePath));
}
