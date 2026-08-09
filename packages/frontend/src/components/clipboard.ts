import { currentFrontendShell } from "../services/frontendShell";
import { writeBrowserClipboardText } from "../shells/browserClipboard";

/** Copies plain text through the owning App Shell. */
export async function copyText(text: string) {
  const shellClipboard = currentFrontendShell()?.clipboard;
  await (shellClipboard?.writeText(text) ?? writeBrowserClipboardText(text));
}
