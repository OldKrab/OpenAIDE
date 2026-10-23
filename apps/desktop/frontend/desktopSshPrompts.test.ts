// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { initializeSshPrompts, type DesktopSshPrompt } from "./desktopSshPrompts";

afterEach(() => {
  document.body.innerHTML = "";
  vi.restoreAllMocks();
});

const password: DesktopSshPrompt = { id: 1, computer: "devbox", text: "me@devbox's password:", kind: "secret" };
const hostKey: DesktopSshPrompt = { id: 2, computer: "devbox", text: "Are you sure you want to continue connecting (yes/no)?", kind: "yesNo" };

async function setUp(pending: DesktopSshPrompt[] = []) {
  let emit: (prompt: DesktopSshPrompt) => void = () => undefined;
  const invoke = vi.fn(async (command: string) => (command === "desktop_ssh_prompts" ? pending : true));
  const listen = vi.fn(async (_event: string, listener: (event: { payload: DesktopSshPrompt }) => void) => {
    emit = (payload) => listener({ payload });
    return () => undefined;
  });
  vi.spyOn(console, "info").mockImplementation(() => undefined);
  initializeSshPrompts({ invoke, listen });
  await vi.waitFor(() => expect(invoke).toHaveBeenCalledWith("desktop_ssh_prompts"));
  return { invoke, emit: (prompt: DesktopSshPrompt) => emit(prompt) };
}

const prompt = () => document.querySelector<HTMLFormElement>(".desktop-ssh-prompt");
const press = (label: string) => [...prompt()!.querySelectorAll("button")].find((button) => button.textContent === label)!.click();

it("asks for a password that was pending before the WebView listened, and logs no answer", async () => {
  const { invoke } = await setUp([password]);

  expect(prompt()?.textContent).toContain("me@devbox's password:");
  const input = prompt()!.querySelector("input")!;
  expect(input.type).toBe("password");
  input.value = "hunter2";
  press("Continue");

  expect(invoke).toHaveBeenCalledWith("desktop_ssh_prompt_answer", { id: 1, answer: "hunter2" });
  expect(prompt()).toBeNull();
  await vi.waitFor(() => expect(console.info).toHaveBeenCalledWith(expect.stringContaining("desktop_ssh_prompt_closed prompt_id=1")));
  expect(JSON.stringify(vi.mocked(console.info).mock.calls)).not.toContain("hunter2");
});

it("shows one question at a time, ignores a repeat, and answers a host key question", async () => {
  const { invoke, emit } = await setUp();
  emit(hostKey);
  emit(password);
  emit(hostKey);

  expect(document.querySelectorAll(".desktop-ssh-prompt")).toHaveLength(1);
  press("No");
  expect(invoke).toHaveBeenCalledWith("desktop_ssh_prompt_answer", { id: 2, answer: "no" });
  expect(prompt()?.querySelector("input")).not.toBeNull();
  press("Cancel");

  expect(invoke).toHaveBeenCalledWith("desktop_ssh_prompt_answer", { id: 1, answer: null });
  expect(prompt()).toBeNull();
});

it("dismisses a notice without answering ssh", async () => {
  const { invoke, emit } = await setUp();
  emit({ id: 3, computer: "devbox", text: "Host key changed.", kind: "notice" });

  expect(prompt()?.textContent).toContain("Message from devbox");
  press("OK");

  expect(prompt()).toBeNull();
  expect(invoke).not.toHaveBeenCalledWith("desktop_ssh_prompt_answer", expect.anything());
});
