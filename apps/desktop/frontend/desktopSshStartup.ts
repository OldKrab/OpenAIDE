type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;

type SshFailure = {
  computerId: string;
  kind: "updateRequired" | "failed";
  runningVersion?: string;
};

/**
 * After a failed start, offers the one way past an SSH Computer whose App
 * Server is too old for this app: replacing it. That App Server refuses this
 * client, so the warning cannot list what it runs; it names what may be lost
 * and asks once more before the shell replaces it on a new start.
 */
export async function offerSshUpdateAfterFailure({
  invoke,
  root,
  computer,
}: {
  invoke: Invoke;
  root: HTMLElement;
  computer: string;
}) {
  let failure: SshFailure | null;
  try {
    failure = await invoke("desktop_ssh_failure") as SshFailure | null;
  } catch {
    console.warn("desktop_ssh_failure_load_completed outcome=failure error_kind=native_command");
    return;
  }
  if (failure?.kind !== "updateRequired") return;
  const update = root.querySelector<HTMLButtonElement>(".desktop-startup-update");
  const label = root.querySelector<HTMLElement>(".desktop-startup-label");
  if (!update || !label) return;
  update.hidden = false;
  let confirming = false;
  update.onclick = () => {
    if (!confirming) {
      confirming = true;
      label.textContent = `Updating restarts OpenAIDE on ${computer}. Other apps connected to it disconnect, and agents working there stop.`;
      update.textContent = "Update and restart";
      return;
    }
    update.disabled = true;
    update.textContent = "Updating…";
    console.info("desktop_ssh_update_requested source=startup");
    // Nothing is connected on this page: the shell asks for a reload, whose start applies the update.
    // The shell's own logs carry the outcome.
    void invoke("desktop_ssh_update").then((outcome) => {
      if (outcome === "reload") window.location.reload();
    }, (error: unknown) => {
      update.disabled = false;
      update.textContent = "Update and restart";
      label.textContent = typeof error === "string" ? error : "OpenAIDE could not update that computer.";
      console.warn("desktop_ssh_update_requested outcome=failure error_kind=native_command");
    });
  };
}
