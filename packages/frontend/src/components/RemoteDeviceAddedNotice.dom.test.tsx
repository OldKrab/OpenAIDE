// @vitest-environment jsdom

import { act } from "react";
import { createRoot, type Root } from "react-dom/client";
import { afterEach, expect, it, vi } from "vitest";
import type { DeviceCollectionSnapshot } from "@openaide/app-server-client";
import { RemoteDeviceAddedNotice } from "./RemoteDeviceAddedNotice";

(globalThis as typeof globalThis & { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true;

let root: Root | undefined;

afterEach(() => {
  act(() => root?.unmount());
  root = undefined;
  document.body.replaceChildren();
});

const tablet = { deviceId: "device_1", name: "Tablet", addedAtMs: 1 };
const phone = { deviceId: "device_2", name: "Phone", addedAtMs: 2, addedBy: "workstation" };

it("stays quiet for the devices already paired when this client connected", async () => {
  await render({ remoteAccess: "on", serverName: "workstation", devices: [tablet] });

  expect(document.querySelector('[role="status"]')).toBeNull();
});

it("announces a device that joins later and opens the Devices page on review", async () => {
  const onReview = vi.fn();
  await render({ remoteAccess: "on", serverName: "workstation", devices: [tablet] }, onReview);
  await render({ remoteAccess: "on", serverName: "workstation", devices: [tablet, phone] }, onReview);

  expect(document.querySelector('[role="status"]')?.textContent).toContain("Phone can now use this OpenAIDE, added from workstation.");

  await act(async () => [...document.querySelectorAll("button")].find((button) => button.textContent === "Review devices")!.click());
  expect(onReview).toHaveBeenCalledOnce();
  expect(document.querySelector('[role="status"]')).toBeNull();
});

it("withdraws the announcement when that device is removed", async () => {
  await render({ remoteAccess: "on", serverName: "workstation" });
  await render({ remoteAccess: "on", serverName: "workstation", devices: [phone] });
  await render({ remoteAccess: "off", serverName: "workstation" });

  expect(document.querySelector('[role="status"]')).toBeNull();
});

async function render(devices: DeviceCollectionSnapshot, onReview = () => undefined) {
  if (!root) {
    const container = document.createElement("div");
    document.body.append(container);
    root = createRoot(container);
  }
  await act(async () => root!.render(<RemoteDeviceAddedNotice devices={devices} onReview={onReview} />));
}
