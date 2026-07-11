export const DEVELOPER_SETTINGS_UNLOCK_KEY = "openaide.developerSettingsUnlocked";

export type DeveloperSettingsStore = {
  get<T>(key: string, defaultValue: T): T;
  update(key: string, value: boolean): PromiseLike<void> | void;
};

export async function unlockDeveloperSettings(store: Pick<DeveloperSettingsStore, "update">) {
  await store.update(DEVELOPER_SETTINGS_UNLOCK_KEY, true);
}

export function developerSettingsVisible(store?: Pick<DeveloperSettingsStore, "get">) {
  return (
    envFlag(process.env.OPENAIDE_DEVELOPER_SETTINGS) ||
    envFlag(process.env.OPENAIDE_ACP_TRACE) ||
    Boolean(store?.get<boolean>(DEVELOPER_SETTINGS_UNLOCK_KEY, false))
  );
}

function envFlag(value: string | undefined) {
  return ["1", "true", "yes", "on"].includes(String(value ?? "").trim().toLowerCase());
}
