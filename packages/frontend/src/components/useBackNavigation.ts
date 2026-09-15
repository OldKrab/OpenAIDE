import { useEffect, useRef } from "react";

type BackHandler = { priority: number; handle(): void };
const hosts = new WeakMap<Window, BackHandler[]>();

export function registerBackNavigation(host: Window, handle: () => void, priority: number) {
  const handlers = hosts.get(host) ?? [];
  hosts.set(host, handlers);
  const entry = { priority, handle };
  handlers.push(entry);
  const onBack = (event: Event) => {
    const first = handlers.reduce<BackHandler | undefined>((current, candidate) =>
      !current || candidate.priority >= current.priority ? candidate : current, undefined);
    if (event.defaultPrevented || first !== entry) return;
    event.preventDefault();
    entry.handle();
  };
  host.addEventListener("openaide:back", onBack);
  return () => {
    handlers.splice(handlers.indexOf(entry), 1);
    host.removeEventListener("openaide:back", onBack);
  };
}

export function useBackNavigation(enabled: boolean, handle: () => void, priority = 100) {
  const handlerRef = useRef(handle);
  useEffect(() => { handlerRef.current = handle; }, [handle]);
  useEffect(() => {
    if (!enabled || typeof window === "undefined") return;
    return registerBackNavigation(window, () => handlerRef.current(), priority);
  }, [enabled, priority]);
}
