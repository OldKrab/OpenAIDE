// @vitest-environment jsdom
import startupHtml from "../index.html?raw";
import { afterEach, expect, it, vi } from "vitest";
import { offerSshUpdateAfterFailure } from "./desktopSshStartup";

afterEach(() => vi.restoreAllMocks());

function setUp(failure: unknown) {
  document.documentElement.innerHTML = startupHtml;
  vi.spyOn(console, "info").mockImplementation(() => undefined);
  const invoke = vi.fn(async (command: string) => (command === "desktop_ssh_failure" ? failure : undefined));
  return { invoke, root: document.getElementById("root")!, update: document.querySelector<HTMLButtonElement>(".desktop-startup-update")! };
}

it("updates an SSH Computer that is too old only after a second, warned click", async () => {
  const { invoke, root, update } = setUp({ computerId: "ssh-1", kind: "updateRequired", runningVersion: "0.8.0" });
  await offerSshUpdateAfterFailure({ invoke, root, computer: "devbox" });

  expect(update.hidden).toBe(false);
  update.click();
  expect(root.querySelector(".desktop-startup-label")?.textContent).toContain("agents working there stop");
  expect(invoke).not.toHaveBeenCalledWith("desktop_ssh_update");
  update.click();

  expect(invoke).toHaveBeenCalledWith("desktop_ssh_update");
});

it("offers nothing when the start failed for another reason", async () => {
  const { invoke, root, update } = setUp({ computerId: "ssh-1", kind: "failed" });
  await offerSshUpdateAfterFailure({ invoke, root, computer: "devbox" });

  expect(update.hidden).toBe(true);
});
