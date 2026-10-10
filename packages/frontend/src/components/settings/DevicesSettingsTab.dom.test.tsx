// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { DeviceCollectionSnapshot } from "@openaide/app-server-client";
import type { RemoteDeviceIntents } from "../../intents/remoteDeviceIntents";
import { DevicesSettingsTab } from "./DevicesSettingsTab";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

const phone = { deviceId: "device_1", name: "Phone", model: "Example 9", addedAtMs: 1_700_000_000_000, addedBy: "workstation" };

it("lists this computer without a remove action and names the relays before first use", async () => {
  await render({ remoteAccess: "off", serverName: "workstation" });

  expect(document.body.textContent).toContain("workstation");
  expect(document.body.textContent).toContain("This computer");
  expect(document.body.textContent).toContain("No other devices are paired.");
  expect(document.body.textContent).toContain("public relays run by the iroh project");
  expect(document.querySelector('button[aria-label^="Remove"]')).toBeNull();
});

it("shows how a connected device is reached", async () => {
  await render({
    remoteAccess: "on",
    serverName: "workstation",
    devices: [{ ...phone, connection: { path: "relayed", address: "203.0.113.7" } }],
  });

  expect(document.body.textContent).toContain("Connected · through a relay · 203.0.113.7");
  expect(document.body.textContent).toContain("from workstation");
  expect(document.body.textContent).not.toContain("public relays run by the iroh project");
});

it("shows a single-use pairing code, withdraws it on close, and reports the device that used it", async () => {
  const intents = fakeIntents();
  const snapshot: DeviceCollectionSnapshot = { remoteAccess: "on", serverName: "workstation" };
  await render(snapshot, intents);

  await act(async () => button("Show code").click());

  expect(intents.createInvite).toHaveBeenCalledOnce();
  expect(document.querySelector("svg.device-qr")).not.toBeNull();
  expect(document.querySelector(".device-code")?.textContent).toBe("OAI1 ABCD EFGH");

  await render({ ...snapshot, devices: [phone] }, intents);
  expect(document.body.textContent).toContain("Phone can now use this OpenAIDE.");
  expect(document.querySelector("svg.device-qr")).toBeNull();

  await act(async () => button("Done").click());
  expect(intents.cancelInvite).toHaveBeenCalledOnce();
});

it("asks for confirmation naming the device and the App Server before adding by code", async () => {
  const intents = fakeIntents();
  await render({ remoteAccess: "off", serverName: "workstation" }, intents);

  await act(async () => button("Enter code").click());
  const input = document.querySelector<HTMLTextAreaElement>('textarea[aria-label="Code from the device"]')!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(input, " OAJ1XYZ ");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => button("Continue").click());

  expect(intents.previewJoinRequest).toHaveBeenCalledWith("OAJ1XYZ");
  expect(intents.approveJoinRequest).not.toHaveBeenCalled();
  expect(document.body.textContent).toContain("Add Phone?");
  expect(document.body.textContent).toContain("Connects to");
  expect(document.body.textContent).toContain("not verified");

  await act(async () => button("Add device").click());
  expect(intents.approveJoinRequest).toHaveBeenCalledWith("OAJ1XYZ");
  expect(document.querySelector('[role="dialog"]')).toBeNull();
});

it("keeps the code entry open with the App Server's reason when a code is refused", async () => {
  const intents = fakeIntents();
  intents.previewJoinRequest.mockRejectedValue(new Error("This is not a device code."));
  await render({ remoteAccess: "off", serverName: "workstation" }, intents);

  await act(async () => button("Enter code").click());
  const input = document.querySelector<HTMLTextAreaElement>("textarea")!;
  await act(async () => {
    Object.getOwnPropertyDescriptor(HTMLTextAreaElement.prototype, "value")!.set!.call(input, "nope");
    input.dispatchEvent(new Event("input", { bubbles: true }));
  });
  await act(async () => button("Continue").click());

  expect(document.querySelector('[role="alert"]')?.textContent).toBe("This is not a device code.");
  expect(document.querySelector("textarea")).not.toBeNull();
});

it("removes a device only after confirmation", async () => {
  const intents = fakeIntents();
  await render({ remoteAccess: "on", serverName: "workstation", devices: [phone] }, intents);

  await act(async () => document.querySelector<HTMLButtonElement>('button[aria-label="Remove Phone"]')!.click());
  expect(intents.remove).not.toHaveBeenCalled();
  await act(async () => button("Remove device").click());

  expect(intents.remove).toHaveBeenCalledWith("device_1");
});

async function render(devices: DeviceCollectionSnapshot, intents: RemoteDeviceIntents = fakeIntents()) {
  if (!root) {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  }
  await act(async () => root!.render(<DevicesSettingsTab devices={devices} intents={intents} />));
}

function fakeIntents() {
  return {
    approveJoinRequest: vi.fn(async () => undefined),
    cancelInvite: vi.fn(async () => undefined),
    createInvite: vi.fn(async () => ({ code: "OAI1ABCDEFGH", expiresAtMs: Date.now() + 600_000 })),
    previewJoinRequest: vi.fn(async () => ({
      deviceId: "device_1",
      name: "Phone",
      model: "Example 9",
      serverName: "workstation",
      alreadyTrusted: false,
    })),
    remove: vi.fn(async () => undefined),
  };
}

function button(label: string) {
  const match = [...document.querySelectorAll("button")].find((candidate) => candidate.textContent?.trim() === label);
  if (!match) throw new Error(`Button not found: ${label}`);
  return match;
}
