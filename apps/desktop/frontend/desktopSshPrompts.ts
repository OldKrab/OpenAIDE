/** One question from `ssh`, relayed by the Desktop's askpass broker (ADR-0069). */
export type DesktopSshPrompt = {
  id: number;
  /** The SSH Computer's name in this Desktop. */
  computer: string;
  /** `ssh`'s own wording, such as a password request or a host key fingerprint. */
  text: string;
  kind: "secret" | "yesNo" | "notice";
};

type Invoke = (command: string, args?: Record<string, unknown>) => Promise<unknown>;
type Listen = (
  event: string,
  listener: (event: { payload: DesktopSshPrompt }) => void,
) => Promise<() => void>;

/**
 * Shows `ssh` questions one at a time, above the startup screen and the app
 * alike, since `ssh` may ask while the shell starts or while it reconnects a
 * forward. The dialog lives on `document.body`, outside the React root that
 * replaces the startup nodes. Answers go straight back to the broker and are
 * never kept here; logs carry only the prompt id and kind.
 */
export function initializeSshPrompts({
  invoke,
  listen,
  body = document.body,
}: {
  invoke: Invoke;
  listen: Listen;
  body?: HTMLElement;
}) {
  const queue: DesktopSshPrompt[] = [];
  const seen = new Set<number>();
  let shown: { prompt: DesktopSshPrompt; element: HTMLElement; startedAt: number } | undefined;

  const enqueue = (prompt: DesktopSshPrompt) => {
    if (seen.has(prompt.id)) return;
    seen.add(prompt.id);
    queue.push(prompt);
    if (!shown) showNext();
  };

  const finish = (answer: string | null, outcome: "answered" | "declined" | "dismissed") => {
    if (!shown) return;
    const { prompt, element, startedAt } = shown;
    shown = undefined;
    element.remove();
    const durationMs = Math.round(performance.now() - startedAt);
    if (prompt.kind === "notice") {
      console.info(`desktop_ssh_prompt_closed prompt_id=${prompt.id} kind=notice outcome=${outcome} duration_ms=${durationMs}`);
    } else {
      void invoke("desktop_ssh_prompt_answer", { id: prompt.id, answer })
        .then((delivered) => {
          // `false` means `ssh` stopped waiting, such as after the broker's timeout.
          console.info(`desktop_ssh_prompt_closed prompt_id=${prompt.id} kind=${prompt.kind} outcome=${outcome} delivered=${delivered === true} duration_ms=${durationMs}`);
        })
        .catch(() => {
          console.warn(`desktop_ssh_prompt_closed prompt_id=${prompt.id} kind=${prompt.kind} outcome=failure error_kind=answer_delivery duration_ms=${durationMs}`);
        });
    }
    showNext();
  };

  const showNext = () => {
    const prompt = queue.shift();
    if (!prompt) return;
    const element = renderPrompt(prompt, finish);
    shown = { prompt, element, startedAt: performance.now() };
    body.append(element);
    element.querySelector<HTMLElement>("input, button[data-primary]")?.focus();
    console.info(`desktop_ssh_prompt_shown prompt_id=${prompt.id} kind=${prompt.kind}`);
  };

  // Listen first, then read what arrived before; ids drop the overlap.
  void listen("desktop-ssh-prompt", ({ payload }) => enqueue(payload))
    .then(() => invoke("desktop_ssh_prompts"))
    .then((pending) => { for (const prompt of pending as DesktopSshPrompt[]) enqueue(prompt); })
    .catch(() => console.warn("desktop_ssh_prompts_load_completed outcome=failure error_kind=native_command"));
}

function renderPrompt(
  prompt: DesktopSshPrompt,
  finish: (answer: string | null, outcome: "answered" | "declined" | "dismissed") => void,
) {
  const form = document.createElement("form");
  form.className = "desktop-ssh-prompt";
  form.setAttribute("role", "dialog");
  form.setAttribute("aria-modal", "true");
  form.setAttribute("aria-label", `${prompt.computer} asks`);
  const title = document.createElement("strong");
  title.textContent = prompt.kind === "notice" ? `Message from ${prompt.computer}` : `${prompt.computer} asks`;
  const text = document.createElement("pre");
  text.textContent = prompt.text;
  const footer = document.createElement("footer");
  form.append(title, text);

  const button = (label: string, primary: boolean, onClick: () => void) => {
    const element = document.createElement("button");
    element.type = primary ? "submit" : "button";
    element.textContent = label;
    if (primary) element.dataset.primary = "";
    else element.onclick = onClick;
    footer.append(element);
  };

  if (prompt.kind === "secret") {
    const input = document.createElement("input");
    input.type = "password";
    input.autocomplete = "off";
    input.setAttribute("aria-label", "Answer");
    form.append(input);
    button("Cancel", false, () => finish(null, "declined"));
    button("Continue", true, () => undefined);
    form.onsubmit = (event) => {
      event.preventDefault();
      finish(input.value, "answered");
    };
  } else if (prompt.kind === "yesNo") {
    button("No", false, () => finish("no", "declined"));
    button("Yes", true, () => undefined);
    form.onsubmit = (event) => {
      event.preventDefault();
      finish("yes", "answered");
    };
  } else {
    button("OK", true, () => undefined);
    form.onsubmit = (event) => {
      event.preventDefault();
      finish(null, "dismissed");
    };
  }
  form.onkeydown = (event) => {
    if (event.key !== "Escape") return;
    event.preventDefault();
    finish(prompt.kind === "yesNo" ? "no" : null, prompt.kind === "notice" ? "dismissed" : "declined");
  };
  form.append(footer);
  return form;
}
