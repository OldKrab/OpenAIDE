export type ConnectionSnapshot = {
  /** A paired or SSH computer, rather than this phone, is the selected workspace. */
  remote: boolean;
  /** This phone is paired with a computer, whether or not it is selected. */
  paired: boolean;
  /** The name of the selected computer; empty when a paired one gave none. */
  computer: string;
  /** SSH Computers this phone starts the App Server on (ADR-0069); absent in older shells. */
  sshComputers?: PhoneSshComputer[];
  /** What the phone learned starting the App Server on the SSH Computer in use. */
  ssh?: PhoneSshStatus | null;
  /** This build pins a Linux App Server release, so it can add SSH Computers. */
  sshAvailable?: boolean;
  /** The outcome of the last Stop of an SSH Computer, for the Remote section. */
  sshNotice?: string;
  busy: boolean;
  notice: string;
  termux: boolean;
  permission: boolean;
  background: boolean;
  appBattery: boolean;
  termuxBattery: boolean;
  batterySaver: boolean;
  notifications: boolean;
  checks: Record<string, boolean | string> | null;
};

/** `destination` is `user@host`, with a port when it is not 22. */
export type PhoneSshComputer = { id: string; name: string; destination: string; current: boolean };

export type PhoneSshStatus = { runningVersion: string; updateAvailable: boolean; suggestLinger: boolean };

export type ConnectionCommand =
  | { action: "state" | "check" | "local" | "install" | "termux" | "get_termux" | "access" | "grant" | "signin" | "battery" | "termux_settings" | "app_settings" | "pair_setup" | "diagnostics" }
  /** `ssh_add` opens the phone's own add flow; `update_ssh` replaces the App Server in use after the Frontend's warning. */
  | { action: "ssh_add" | "update_ssh" }
  | { action: "use_computer" | "forget_computer" | "use_ssh" | "forget_ssh" | "stop_ssh"; id: string }
  | { action: "background"; enabled: boolean };

export type ConnectionSettings = {
  snapshot(): ConnectionSnapshot | undefined;
  subscribe(listener: () => void): () => void;
  execute(command: ConnectionCommand): Promise<void>;
};
