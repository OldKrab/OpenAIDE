// @vitest-environment jsdom

import { act } from "react";
import { afterEach, expect, it, vi } from "vitest";
import { AppServerProtocolError, type DeviceCollectionSnapshot, type OwnRelaySnapshot, type RelayPerson } from "@openaide/app-server-client";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { button, buttons, click, dialog, mountSettings, pageText, settle, type, unmount } from "./settingsTestSupport";

afterEach(() => {
  unmount();
  vi.restoreAllMocks();
});

const phone = { deviceId: "device_1", name: "Phone", addedAtMs: 1_700_000_000_000 };
const relayKey = "https://relay.example.com/?token=secret-of-this-computer";
const newPersonKey = "https://relay.example.com/?token=secret-for-sam";

function intents(overrides: Partial<RemoteDeviceIntents> = {}) {
  return {
    cancelInvite: vi.fn(async () => undefined),
    createInvite: vi.fn(async () => ({ code: "OAI1ABCDEFGH", expiresAtMs: Date.now() + 600_000 })),
    remove: vi.fn(async () => undefined),
    replacementImpact: vi.fn(async () => ({ activeTurns: 0, pendingTaskRequests: 0, openTerminals: 0 })),
    setRelay: vi.fn(async () => ({ removedDevices: 0 })),
    relayPeople: vi.fn(async (): Promise<RelayPerson[]> => []),
    relayAddPerson: vi.fn(async (name: string, canManage: boolean) => ({ person: { name, canManage, thisComputer: false }, key: newPersonKey })),
    relayRevokePerson: vi.fn(async () => undefined),
    ...overrides,
  };
}

function snapshot(ownRelay?: OwnRelaySnapshot, devices = [phone]): DeviceCollectionSnapshot {
  return { remoteAccess: "on", serverName: "workstation", devices, ...(ownRelay ? { ownRelay } : {}) };
}

const remote = (devices: DeviceCollectionSnapshot) => ({ activeTab: "remote" as const, devices, loading: false });
const headings = () => [...document.querySelectorAll("h2")].map((heading) => heading.textContent);
const row = (name: string) => [...document.querySelectorAll("li")].find((item) => item.querySelector("strong")?.textContent === name);

/** Presses a button inside one dialog, where the page may hold a button of the same name. */
async function clickIn(dialogLabel: string, name: string) {
  const match = [...dialog(dialogLabel)?.querySelectorAll<HTMLElement>("button") ?? []]
    .find((candidate) => (candidate.getAttribute("aria-label") ?? candidate.textContent?.trim()) === name);
  if (!match) throw new Error(`Button not found in ${dialogLabel}: ${name}`);
  await act(async () => match.click());
}

function refusal(code: "validationFailed" | "conflict", message: string, field: string) {
  return new AppServerProtocolError({ error: { code, message, target: { field } } });
}

it("says devices go through the free public relays and offers an own relay, with no people to manage", async () => {
  const relay = intents();
  await mountSettings({ deviceIntents: relay, state: remote(snapshot()) });
  await settle();

  expect(headings()).toEqual(expect.arrayContaining(["Devices using workstation", "Relay"]));
  expect(headings()).not.toContain("My relay");
  expect(row("Free public relays")?.textContent).toContain("For devices that can't reach workstation directly");
  expect(buttons("Use own relay")).toHaveLength(1);
  expect(relay.relayPeople).not.toHaveBeenCalled();
});

it("warns that a relay removes the paired devices, keeps the dialog open on the server's refusal, then reports the removal", async () => {
  const logged = [vi.spyOn(console, "info"), vi.spyOn(console, "warn"), vi.spyOn(console, "error"), vi.spyOn(console, "log")];
  const relay = intents({
    setRelay: vi.fn()
      .mockRejectedValueOnce(refusal("validationFailed", "The relay doesn't accept this key", "relay"))
      .mockResolvedValueOnce({ removedDevices: 1 }),
  });
  await mountSettings({ deviceIntents: relay, state: remote(snapshot()) });

  await click("Use own relay");
  const setting = dialog("Use your own relay");
  expect(setting?.textContent).toContain("This removes the paired device. It must be paired again.");
  expect(button("Use relay").hasAttribute("disabled")).toBe(true);

  await type("Relay key", ` ${relayKey} `);
  await click("Use relay");
  await settle();

  expect(relay.setRelay).toHaveBeenLastCalledWith(relayKey);
  expect(dialog("Use your own relay")?.querySelector('[role="alert"]')?.textContent).toBe("The relay doesn't accept this key");
  expect(document.querySelector('[aria-label="Relay key"]')?.getAttribute("aria-invalid")).toBe("true");

  await click("Use relay");
  await settle();

  expect(dialog("Use your own relay")).toBeNull();
  expect(pageText()).toContain("1 device was removed; pair it again.");
  expect(pageText()).not.toContain(relayKey);
  for (const spy of logged) expect(JSON.stringify(spy.mock.calls)).not.toContain("secret-of-this-computer");
});

it("says when the relay refuses this computer and offers a new key or the free relays", async () => {
  const relay = intents();
  await mountSettings({ deviceIntents: relay, state: remote(snapshot({ host: "relay.example.com", canManage: true, access: "refused" })) });
  await settle();

  expect(row("relay.example.com")?.textContent).toContain("The relay no longer accepts this computer.");
  expect(buttons("Enter new key")).toHaveLength(1);
  // A refused key cannot manage the relay either.
  expect(headings()).not.toContain("My relay");
  expect(relay.relayPeople).not.toHaveBeenCalled();

  await click("Use free relays");
  expect(dialog("Use the free relays")?.textContent).toContain("This removes the paired device. It must be paired again.");
  await clickIn("Use the free relays", "Use free relays");
  await settle();

  expect(relay.setRelay).toHaveBeenCalledWith(undefined);
  expect(dialog("Use the free relays")).toBeNull();

  await click("Enter new key");
  expect(dialog("Change relay key")).not.toBeNull();
});

it("lists the relay's people, adds one with a key shown once, and revokes another after confirmation", async () => {
  const people: RelayPerson[] = [
    { name: "workstation", canManage: true, thisComputer: true },
    { name: "Alex", canManage: false, thisComputer: false },
  ];
  const relay = intents({
    relayPeople: vi.fn(async () => people),
    relayAddPerson: vi.fn()
      .mockRejectedValueOnce(refusal("conflict", "Someone named Alex already has a key", "name"))
      .mockImplementation(async (name: string, canManage: boolean) => ({ person: { name, canManage, thisComputer: false }, key: newPersonKey })),
  });
  await mountSettings({ deviceIntents: relay, state: remote(snapshot({ host: "relay.example.com", canManage: true, access: "accepted" })) });
  await settle();

  expect(headings()).toContain("My relay");
  expect(row("workstation")?.textContent).toContain("This computer · Can add and remove people");
  expect(buttons("Revoke workstation")).toHaveLength(0);
  expect(buttons("Revoke Alex")).toHaveLength(1);
  expect(buttons("Change key")).toHaveLength(1);
  expect(relay.relayPeople).toHaveBeenCalledOnce();

  await click("Add person");
  await type("Name", "Alex");
  await clickIn("Add a person", "Add person");
  await settle();
  expect(dialog("Add a person")?.querySelector('[role="alert"]')?.textContent).toBe("Someone named Alex already has a key");
  expect(document.querySelector('[aria-label="Name"]')?.getAttribute("aria-invalid")).toBe("true");

  await type("Name", "Sam");
  const canManage = dialog("Add a person")!.querySelector<HTMLInputElement>('input[type="checkbox"]')!;
  expect(canManage.checked).toBe(false);
  await act(async () => canManage.click());
  await clickIn("Add a person", "Add person");
  await settle();

  expect(relay.relayAddPerson).toHaveBeenLastCalledWith("Sam", true);
  const handover = dialog("Relay key for Sam");
  expect(handover?.querySelector('[aria-label="Relay key text"]')?.textContent).toBe(newPersonKey);
  expect(handover?.querySelector('[aria-label="Relay key as a QR code"]')).not.toBeNull();
  expect(handover?.querySelector(".settings-pairing-path")?.getAttribute("aria-label"))
    .toBe("Give it to Sam, who enters it in Settings, then Remote, then Use own relay");
  expect(relay.relayPeople).toHaveBeenCalledTimes(2);

  await click("Done");
  expect(dialog("Relay key for Sam")).toBeNull();
  expect(pageText()).not.toContain(newPersonKey);

  await click("Revoke Alex");
  expect(relay.relayRevokePerson).not.toHaveBeenCalled();
  expect(dialog("Revoke relay key")?.textContent).toContain("Their computers stop using this relay at once.");
  await clickIn("Revoke relay key", "Revoke");
  await settle();

  expect(relay.relayRevokePerson).toHaveBeenCalledWith("Alex");
  expect(relay.relayPeople).toHaveBeenCalledTimes(3);
});

it("shows why the relay's people could not be read and reads them again on request", async () => {
  const relay = intents({
    relayPeople: vi.fn()
      .mockRejectedValueOnce(new Error("The relay did not answer."))
      .mockResolvedValueOnce([{ name: "workstation", canManage: true, thisComputer: true }]),
  });
  await mountSettings({ deviceIntents: relay, state: remote(snapshot({ host: "relay.example.com", canManage: true, access: "accepted" })) });
  await settle();

  expect(pageText()).toContain("The relay did not answer.");
  await click("Try again");
  await settle();

  expect(pageText()).not.toContain("The relay did not answer.");
  expect(row("workstation")).toBeDefined();
});
