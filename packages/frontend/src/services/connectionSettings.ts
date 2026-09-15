export type ConnectionSnapshot = {
  remote: boolean;
  address: string;
  username: string;
  busy: boolean;
  notice: string;
  termux: boolean;
  permission: boolean;
  background: boolean;
  appBattery: boolean;
  termuxBattery: boolean;
  batterySaver: boolean;
  notifications: boolean;
  boot: boolean;
  checks: Record<string, boolean | string> | null;
  scannedAddress?: string;
  scanSequence?: number;
};

export type ConnectionCommand =
  | { action: "state" | "check" | "local" | "install" | "termux" | "get_termux" | "access" | "grant" | "signin" | "battery" | "termux_settings" | "app_settings" | "repair" | "boot" | "get_boot" | "qr" | "diagnostics" }
  | { action: "background"; enabled: boolean }
  | { action: "remote"; address: string; username: string; password: string };

export type ConnectionSettings = {
  snapshot(): ConnectionSnapshot | undefined;
  subscribe(listener: () => void): () => void;
  execute(command: ConnectionCommand): Promise<void>;
};
