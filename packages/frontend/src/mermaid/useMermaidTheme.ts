import { useSyncExternalStore } from "react";
import type { MermaidTheme } from "./rendererProtocol";

const FALLBACK_THEME: MermaidTheme = {
  background: "rgb(31, 31, 31)",
  border: "rgb(93, 93, 93)",
  highContrast: false,
  muted: "rgb(157, 161, 166)",
  panel: "rgb(37, 37, 38)",
  primary: "rgb(77, 156, 255)",
  text: "rgb(204, 204, 204)",
  fontFamily: "system-ui, sans-serif",
};

let currentTheme = FALLBACK_THEME;
let observer: MutationObserver | undefined;
let initialized = false;
const listeners = new Set<() => void>();

export function useMermaidTheme() {
  return useSyncExternalStore(subscribe, getSnapshot, () => FALLBACK_THEME);
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  if (listeners.size === 1) startWatching();
  return () => {
    listeners.delete(listener);
    if (listeners.size === 0) {
      observer?.disconnect();
      observer = undefined;
    }
  };
}

function getSnapshot() {
  if (typeof document !== "undefined" && !initialized) {
    currentTheme = captureTheme();
    initialized = true;
  }
  return currentTheme;
}

function startWatching() {
  const next = captureTheme();
  const changed = JSON.stringify(next) !== JSON.stringify(currentTheme);
  currentTheme = next;
  initialized = true;
  observer = new MutationObserver(updateTheme);
  observer.observe(document.body, { attributes: true, attributeFilter: ["class", "style"] });
  observer.observe(document.documentElement, { attributes: true, attributeFilter: ["class", "style"] });
  if (changed) queueMicrotask(() => listeners.forEach((listener) => listener()));
}

function updateTheme() {
  const next = captureTheme();
  if (JSON.stringify(next) === JSON.stringify(currentTheme)) return;
  currentTheme = next;
  listeners.forEach((listener) => listener());
}

function captureTheme(): MermaidTheme {
  const probe = document.createElement("span");
  probe.className = "mermaid-theme-probe";
  probe.setAttribute("aria-hidden", "true");
  document.body.append(probe);
  const style = getComputedStyle(probe);
  const theme = {
    background: supportedColor(style.backgroundColor),
    border: supportedColor(style.borderTopColor),
    highContrast: document.body.classList.contains("vscode-high-contrast")
      || document.body.classList.contains("vscode-high-contrast-light"),
    muted: supportedColor(style.borderLeftColor),
    panel: supportedColor(style.borderBottomColor),
    primary: supportedColor(style.borderRightColor),
    text: supportedColor(style.color),
    fontFamily: style.fontFamily,
  };
  probe.remove();
  return theme;
}

/** Mermaid's color library does not parse modern CSS colors such as OKLCH. */
function supportedColor(value: string) {
  const canvas = document.createElement("canvas");
  canvas.width = 1;
  canvas.height = 1;
  const context = canvas.getContext("2d", { willReadFrequently: true });
  if (!context) return value;
  context.clearRect(0, 0, 1, 1);
  context.fillStyle = value;
  context.fillRect(0, 0, 1, 1);
  const [red, green, blue, alpha] = context.getImageData(0, 0, 1, 1).data;
  return `rgba(${red}, ${green}, ${blue}, ${(alpha / 255).toFixed(3)})`;
}
