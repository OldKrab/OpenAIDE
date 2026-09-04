import type { AppShellSecretStore } from "@openaide/app-shell-contracts";

const DATABASE_NAME = "openaide-secure-storage";
const DATABASE_VERSION = 1;
const KEY_STORE = "keys";
const SECRET_STORE = "secrets";
const ENCRYPTION_KEY = "vault-encryption-key";

export type EncryptedSecretRecord = {
  ciphertext: ArrayBuffer;
  iv: ArrayBuffer;
};

export type WebSecretVaultPersistence = {
  encryptionKey(candidate: CryptoKey): Promise<CryptoKey>;
  delete(key: string): Promise<void>;
  read(key: string): Promise<EncryptedSecretRecord | undefined>;
  write(key: string, value: EncryptedSecretRecord): Promise<void>;
};

/** Browser-profile vault. The non-extractable AES key and ciphertext stay in IndexedDB. */
export function createWebSecretStore(
  browserCrypto: Crypto = window.crypto,
  indexedDb: IDBFactory = window.indexedDB,
): AppShellSecretStore {
  return createEncryptedSecretStore(browserCrypto, indexedDbPersistence(indexedDb));
}

export function createEncryptedSecretStore(
  browserCrypto: Crypto,
  persistence: WebSecretVaultPersistence,
): AppShellSecretStore {
  let encryptionKey: Promise<CryptoKey> | undefined;
  const key = () => encryptionKey ??= browserCrypto.subtle
    .generateKey({ name: "AES-GCM", length: 256 }, false, ["encrypt", "decrypt"])
    .then((candidate) => persistence.encryptionKey(candidate));
  const encoder = new TextEncoder();
  const decoder = new TextDecoder();

  return {
    delete: (storageKey) => persistence.delete(storageKey),
    async get(storageKey) {
      const record = await persistence.read(storageKey);
      if (!record) return undefined;
      const plaintext = await browserCrypto.subtle.decrypt(
        {
          name: "AES-GCM",
          iv: record.iv,
          additionalData: encoder.encode(storageKey),
        },
        await key(),
        record.ciphertext,
      );
      return decoder.decode(plaintext);
    },
    async store(storageKey, value) {
      const iv = browserCrypto.getRandomValues(new Uint8Array(12));
      const ciphertext = await browserCrypto.subtle.encrypt(
        {
          name: "AES-GCM",
          iv,
          additionalData: encoder.encode(storageKey),
        },
        await key(),
        encoder.encode(value),
      );
      await persistence.write(storageKey, {
        ciphertext,
        iv: iv.buffer.slice(iv.byteOffset, iv.byteOffset + iv.byteLength),
      });
    },
  };
}

function indexedDbPersistence(indexedDb: IDBFactory): WebSecretVaultPersistence {
  let database: Promise<IDBDatabase> | undefined;
  const getDatabase = () => database ??= openDatabase(indexedDb);
  return {
    async encryptionKey(candidate) {
      const db = await getDatabase();
      return new Promise<CryptoKey>((resolve, reject) => {
        const transaction = db.transaction(KEY_STORE, "readwrite");
        const store = transaction.objectStore(KEY_STORE);
        const read = store.get(ENCRYPTION_KEY);
        read.onerror = () => reject(read.error ?? new Error("Secure storage key could not be read."));
        read.onsuccess = () => {
          if (read.result instanceof CryptoKey) {
            resolve(read.result);
            return;
          }
          const write = store.add(candidate, ENCRYPTION_KEY);
          write.onerror = () => reject(write.error ?? new Error("Secure storage key could not be saved."));
          write.onsuccess = () => resolve(candidate);
        };
      });
    },
    async delete(key) {
      await databaseRequest(getDatabase(), SECRET_STORE, "readwrite", (store) => store.delete(key));
    },
    async read(key) {
      return databaseRequest<EncryptedSecretRecord | undefined>(
        getDatabase(),
        SECRET_STORE,
        "readonly",
        (store) => store.get(key),
      );
    },
    async write(key, value) {
      await databaseRequest(getDatabase(), SECRET_STORE, "readwrite", (store) => store.put(value, key));
    },
  };
}

function openDatabase(indexedDb: IDBFactory) {
  return new Promise<IDBDatabase>((resolve, reject) => {
    const request = indexedDb.open(DATABASE_NAME, DATABASE_VERSION);
    request.onerror = () => reject(request.error ?? new Error("Secure storage could not be opened."));
    request.onupgradeneeded = () => {
      const db = request.result;
      if (!db.objectStoreNames.contains(KEY_STORE)) db.createObjectStore(KEY_STORE);
      if (!db.objectStoreNames.contains(SECRET_STORE)) db.createObjectStore(SECRET_STORE);
    };
    request.onsuccess = () => resolve(request.result);
  });
}

async function databaseRequest<T = undefined>(
  database: Promise<IDBDatabase>,
  storeName: string,
  mode: IDBTransactionMode,
  request: (store: IDBObjectStore) => IDBRequest,
) {
  const db = await database;
  return new Promise<T>((resolve, reject) => {
    const operation = request(db.transaction(storeName, mode).objectStore(storeName));
    operation.onerror = () => reject(operation.error ?? new Error("Secure storage operation failed."));
    operation.onsuccess = () => resolve(operation.result as T);
  });
}
