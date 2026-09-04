import { describe, expect, it, vi } from "vitest";
import { webcrypto } from "node:crypto";
import {
  createEncryptedSecretStore,
  createWebSecretStore,
  type EncryptedSecretRecord,
  type WebSecretVaultPersistence,
} from "./webSecretVault";

function memoryPersistence(): WebSecretVaultPersistence & {
  records: Map<string, EncryptedSecretRecord>;
} {
  const records = new Map<string, EncryptedSecretRecord>();
  let encryptionKey: CryptoKey | undefined;
  return {
    records,
    async encryptionKey(candidate) {
      encryptionKey ??= candidate;
      return encryptionKey;
    },
    async delete(key) { records.delete(key); },
    async read(key) { return records.get(key); },
    async write(key, value) { records.set(key, value); },
  };
}

describe("Web encrypted secret store", () => {
  it("does not open IndexedDB until a secret operation needs it", () => {
    const open = vi.fn(() => ({} as IDBOpenDBRequest));

    createWebSecretStore(webcrypto as unknown as Crypto, { open } as unknown as IDBFactory);

    expect(open).not.toHaveBeenCalled();
  });

  it("persists only encrypted bytes and decrypts them through the non-extractable key", async () => {
    const persistence = memoryPersistence();
    const store = createEncryptedSecretStore(webcrypto as unknown as Crypto, persistence);

    await store.store("openaide.agent.codex.env.OPENAI_API_KEY", "test-api-key");

    const persisted = persistence.records.get("openaide.agent.codex.env.OPENAI_API_KEY");
    expect(persisted).toBeDefined();
    expect(JSON.stringify(persisted)).not.toContain("test-api-key");
    await expect(store.get("openaide.agent.codex.env.OPENAI_API_KEY")).resolves.toBe("test-api-key");
  });

  it("binds ciphertext to its storage key and supports deletion", async () => {
    const persistence = memoryPersistence();
    const store = createEncryptedSecretStore(webcrypto as unknown as Crypto, persistence);
    await store.store("first", "secret");
    persistence.records.set("second", persistence.records.get("first")!);

    await expect(store.get("second")).rejects.toThrow();
    await store.delete("first");
    await expect(store.get("first")).resolves.toBeUndefined();
  });
});
