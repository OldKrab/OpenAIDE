const variants = [
  { key: "A", name: "Workbench form" },
  { key: "B", name: "Conversational fields" },
  { key: "C", name: "Compact worksheet" },
];

const params = new URLSearchParams(window.location.search);
let variant = variants.some((item) => item.key === params.get("variant")) ? params.get("variant") : "A";
let state = ["pending", "error", "sending", "resolved", "closed"].includes(params.get("state"))
  ? params.get("state")
  : "pending";

const root = document.querySelector("#question-root");
const stateSelect = document.querySelector("#state-select");
const variantLabel = document.querySelector("#variant-label");

function choiceFields(layout) {
  const error = state === "error";
  const disabled = state === "sending";
  const checked = error ? "" : "checked";
  const textValue = error ? "Q" : "Question";
  return `
    <fieldset class="choice-field ${error ? "field-invalid" : ""}" ${disabled ? "disabled" : ""}>
      <legend><span class="field-number">1</span><span><strong>Which implementation scope should we start with?</strong><small>Choose one approach.</small></span></legend>
      <div class="choice-list ${layout}">
        <label><input type="radio" name="scope" value="form" ${checked}><span><strong>Form only</strong><small>Support structured questions first.</small></span></label>
        <label><input type="radio" name="scope" value="both"><span><strong>Form and URL</strong><small>Include browser-based flows.</small></span></label>
        <label><input type="radio" name="scope" value="url"><span><strong>URL only</strong><small>Prioritize authentication flows.</small></span></label>
      </div>
      ${error ? '<p class="field-error" role="alert">Choose an implementation scope.</p>' : ""}
    </fieldset>
    <label class="text-field ${error ? "field-invalid" : ""}">
      <span class="field-label"><span class="field-number">2</span><span><strong>What should the UI call this interaction?</strong><small>Shown in Task history after resolution.</small></span></span>
      <input type="text" value="${textValue}" ${disabled ? "disabled" : ""} aria-invalid="${error}">
      ${error ? '<span class="field-error" role="alert">Use at least 3 characters.</span>' : ""}
    </label>
    <label class="select-field">
      <span class="field-label"><span class="field-number">3</span><span><strong>How many answers should preview?</strong><small>Longer responses remain expandable.</small></span></span>
      <select ${disabled ? "disabled" : ""}>
        <option>Up to three</option>
        <option>Only the first</option>
        <option>Show every answer</option>
      </select>
    </label>`;
}

function actions() {
  if (state === "sending") {
    return `<div class="question-actions"><button class="primary" disabled><span class="spinner" aria-hidden="true"></span>Submitting</button><button disabled>Cancel</button></div>`;
  }
  return `<div class="question-actions"><button class="primary" type="submit">Submit</button><button data-action="cancel" type="button">Cancel</button></div>`;
}

function resolvedContent(className = "") {
  return `
    <section class="question-resolved ${className}" aria-label="Answered question">
      <header><span class="question-symbol answered" aria-hidden="true">✓</span><span><strong>Question answered</strong><small>5 answers submitted</small></span><button type="button" data-action="toggle-answers">Show all</button></header>
      <div class="answer-preview">
        <dl><div><dt>Which implementation scope should we start with?</dt><dd>Form only</dd></div><div><dt>What should the UI call this interaction?</dt><dd>Question</dd></div><div><dt>How many answers should preview?</dt><dd>Up to three</dd></div><div class="additional-answer" hidden><dt>How should background Tasks signal this?</dt><dd>Waiting</dd></div><div class="additional-answer" hidden><dt>Should Cancel require confirmation?</dt><dd>No</dd></div></dl>
      </div>
    </section>`;
}

function closedContent(className = "") {
  return `
    <section class="question-resolved closed ${className}" aria-label="Closed question">
      <header><span class="question-symbol" aria-hidden="true">×</span><span><strong>Question closed</strong><small>Closed without response</small></span></header>
      <p>Codex needs your input to continue.</p>
    </section>`;
}

function variantA() {
  if (state === "resolved") return resolvedContent("variant-a");
  if (state === "closed") return closedContent("variant-a");
  return `
    <form class="question-panel variant-a" novalidate>
      <header class="question-heading"><span class="question-symbol" aria-hidden="true">?</span><span><strong>Question</strong><small>Codex needs your input to continue.</small></span><span class="question-state">Waiting</span></header>
      <div class="question-fields">${choiceFields("stacked")}</div>
      ${actions()}
    </form>`;
}

function variantB() {
  if (state === "resolved") return resolvedContent("variant-b");
  if (state === "closed") return closedContent("variant-b");
  return `
    <form class="question-document variant-b" novalidate>
      <div class="document-intro"><span class="question-symbol" aria-hidden="true">?</span><div><p class="eyebrow">Question</p><h2>Codex needs your input to continue.</h2><p>Your answers are submitted together and saved in Task history.</p></div></div>
      <div class="document-fields">${choiceFields("segmented")}</div>
      <footer>${actions()}</footer>
    </form>`;
}

function variantC() {
  if (state === "resolved") return resolvedContent("variant-c");
  if (state === "closed") return closedContent("variant-c");
  return `
    <form class="question-worksheet variant-c" novalidate>
      <header><div><span class="eyebrow">Question · 3 fields</span><strong>Codex needs your input to continue.</strong></div><span class="question-state">Waiting</span></header>
      <div class="worksheet-body">${choiceFields("compact")}</div>
      <footer><span>All required fields must be valid.</span>${actions()}</footer>
    </form>`;
}

function render() {
  document.body.dataset.variant = variant;
  document.body.dataset.state = state;
  root.innerHTML = variant === "A" ? variantA() : variant === "B" ? variantB() : variantC();
  const current = variants.find((item) => item.key === variant);
  variantLabel.textContent = `${current.key} · ${current.name}`;
  stateSelect.value = state;
  bindQuestionActions();
  window.requestAnimationFrame(() => {
    const messageList = document.querySelector(".message-list");
    messageList.scrollTop = messageList.scrollHeight;
  });
}

function updateUrl(nextVariant, nextState) {
  variant = nextVariant;
  state = nextState;
  const next = new URL(window.location.href);
  next.searchParams.set("variant", variant);
  next.searchParams.set("state", state);
  window.history.replaceState({}, "", next);
  render();
}

function cycleVariant(offset) {
  const currentIndex = variants.findIndex((item) => item.key === variant);
  const nextIndex = (currentIndex + offset + variants.length) % variants.length;
  updateUrl(variants[nextIndex].key, state);
}

function bindQuestionActions() {
  root.querySelector("form")?.addEventListener("submit", (event) => {
    event.preventDefault();
    if (state === "error") {
      updateUrl(variant, "pending");
      return;
    }
    updateUrl(variant, "sending");
    window.setTimeout(() => updateUrl(variant, "resolved"), 700);
  });
  root.querySelector('[data-action="cancel"]')?.addEventListener("click", () => updateUrl(variant, "closed"));
  root.querySelector('[data-action="toggle-answers"]')?.addEventListener("click", (event) => {
    const extraAnswers = [...root.querySelectorAll(".additional-answer")];
    const shouldShow = extraAnswers.some((answer) => answer.hidden);
    extraAnswers.forEach((answer) => { answer.hidden = !shouldShow; });
    event.currentTarget.textContent = shouldShow ? "Show less" : "Show all";
  });
}

document.querySelector("#previous-variant").addEventListener("click", () => cycleVariant(-1));
document.querySelector("#next-variant").addEventListener("click", () => cycleVariant(1));
stateSelect.addEventListener("change", () => updateUrl(variant, stateSelect.value));
window.addEventListener("keydown", (event) => {
  const target = event.target;
  if (target.matches("input, textarea, select, [contenteditable='true']")) return;
  if (event.key === "ArrowLeft") cycleVariant(-1);
  if (event.key === "ArrowRight") cycleVariant(1);
});

render();
