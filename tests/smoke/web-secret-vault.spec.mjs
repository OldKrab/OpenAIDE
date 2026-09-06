import { readFile } from "node:fs/promises";
import http from "node:http";
import { test, expect } from "@playwright/test";
import { transformWithOxc } from "vite";

let server;
let baseUrl;

test.beforeAll(async () => {
  const filename = new URL("../../apps/web/frontend/webSecretVault.ts", import.meta.url);
  const { code } = await transformWithOxc(await readFile(filename, "utf8"), filename.pathname);
  server = http.createServer((request, response) => {
    response.setHeader("Content-Type", request.url === "/vault.js" ? "text/javascript" : "text/html");
    response.end(request.url === "/vault.js" ? code : "<!doctype html><title>Vault transaction regression</title>");
  });
  await new Promise((resolve) => server.listen(0, "127.0.0.1", resolve));
  baseUrl = `http://127.0.0.1:${server.address().port}`;
});

test.afterAll(async () => {
  server?.closeAllConnections();
  if (server) await new Promise((resolve) => server.close(resolve));
});

test.beforeEach(async ({ page }) => {
  await page.goto(baseUrl);
  await page.evaluate(async () => {
    const { createWebSecretStore } = await import("/vault.js");
    window.createVault = createWebSecretStore;
    window.vault = createWebSecretStore();
    window.vaultEvents = [];
    const originalTransaction = IDBDatabase.prototype.transaction;
    IDBDatabase.prototype.transaction = function (...args) {
      const transaction = Reflect.apply(originalTransaction, this, args);
      if (args[0] === "secrets" && args[1] === "readwrite") {
        transaction.addEventListener("complete", () => window.vaultEvents.push("committed"));
      }
      return transaction;
    };
    // Abort the real transaction after its request succeeds. Request success is
    // not a commit: storage/quota failures can still abort the transaction here.
    for (const method of ["add", "put", "delete", "get"]) {
      const original = IDBObjectStore.prototype[method];
      IDBObjectStore.prototype[method] = function (...args) {
        const request = Reflect.apply(original, this, args);
        if (window.abortVaultRequest === `${this.name}.${method}`) {
          window.abortVaultRequest = undefined;
          request.addEventListener("success", () => this.transaction.abort());
        }
        return request;
      };
    }
  });
});

test("a saved credential is acknowledged only after its transaction commits", async ({ page }) => {
  const events = await page.evaluate(async () => {
    await window.vault.store("credential", "secret");
    window.vaultEvents.push("acknowledged");
    return window.vaultEvents;
  });
  expect(events).toEqual(["committed", "acknowledged"]);
  await page.reload();
  expect(await page.evaluate(async () => {
    const { createWebSecretStore } = await import("/vault.js");
    return createWebSecretStore().get("credential");
  })).toBe("secret");
});

for (const method of ["put", "delete", "get"]) {
  test(`a successful credential ${method} request still rejects a later transaction abort`, async ({ page }) => {
    const outcome = await page.evaluate(async (method) => {
      await window.vault.store("credential", "original");
      window.abortVaultRequest = `secrets.${method}`;
      const operation = method === "put"
        ? window.vault.store("credential", "replacement")
        : method === "delete"
          ? window.vault.delete("credential")
          : window.vault.get("credential");
      const result = await operation.then(() => "acknowledged", (error) => error.name);
      return { result, retained: await window.createVault().get("credential") };
    }, method);
    expect(outcome).toEqual({ result: "AbortError", retained: "original" });
  });
}

test("an aborted vault-key transaction fails the save and can be retried", async ({ page }) => {
  const outcome = await page.evaluate(async () => {
    window.abortVaultRequest = "keys.add";
    const first = await window.vault.store("credential", "uncommitted")
      .then(() => "acknowledged", (error) => error.name);
    const afterAbort = await window.createVault().get("credential");
    await window.vault.store("credential", "retried");
    return { first, afterAbort, retried: await window.createVault().get("credential") };
  });
  expect(outcome).toEqual({ first: "AbortError", afterAbort: undefined, retried: "retried" });
});

test("concurrent vault instances share the committed key", async ({ page }) => {
  const values = await page.evaluate(async () => {
    await Promise.all([
      window.vault.store("first", "first secret"),
      window.createVault().store("second", "second secret"),
    ]);
    const reopened = window.createVault();
    return Promise.all([reopened.get("first"), reopened.get("second")]);
  });
  expect(values).toEqual(["first secret", "second secret"]);
});

test("an aborted existing-key read rejects without poisoning the vault", async ({ page }) => {
  const outcome = await page.evaluate(async () => {
    await window.vault.store("credential", "original");
    const reopened = window.createVault();
    window.abortVaultRequest = "keys.get";
    const first = await reopened.get("credential").then(() => "acknowledged", (error) => error.name);
    return { first, retried: await reopened.get("credential") };
  });
  expect(outcome).toEqual({ first: "AbortError", retried: "original" });
});
