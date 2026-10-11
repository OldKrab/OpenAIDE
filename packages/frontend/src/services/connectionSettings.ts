export type ConnectionSnapshot = {
  /** The paired computer, rather than this phone, is the selected workspace. */
  remote: boolean;
  /** This phone is paired with a computer, whether or not it is selected. */
  paired: boolean;
  /** The name the paired computer reported; empty when it gave none. */
  computer: string;
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

export type ConnectionCommand =
  | { action: "state" | "check" | "local" | "install" | "termux" | "get_termux" | "access" | "grant" | "signin" | "battery" | "termux_settings" | "app_settings" | "paired" | "pair_setup" | "forget" | "diagnostics" }
  | { action: "background"; enabled: boolean };

export type ConnectionSettings = {
  snapshot(): ConnectionSnapshot | undefined;
  subscribe(listener: () => void): () => void;
  execute(command: ConnectionCommand): Promise<void>;
};
