import { useCallback, useEffect, useState } from "react";
import type { PrototypeDefinition } from "./prototypeApi";

/** Renders and switches review candidates without reloading the prototype application. */
export function PrototypeView({ definition }: { definition: PrototypeDefinition }) {
  const [requestedVariant, setRequestedVariant] = useState(variantFromUrl);
  const fallbackKey = definition.defaultVariant ?? definition.variants[0]?.key;
  const current = definition.variants.find((variant) => variant.key === requestedVariant)
    ?? definition.variants.find((variant) => variant.key === fallbackKey)
    ?? definition.variants[0];

  const selectVariant = useCallback((key: string) => {
    const url = new URL(window.location.href);
    url.searchParams.set("variant", key);
    window.history.replaceState(null, "", url);
    setRequestedVariant(key);
  }, []);

  useEffect(() => {
    const handleHistoryChange = () => setRequestedVariant(variantFromUrl());
    window.addEventListener("popstate", handleHistoryChange);
    return () => window.removeEventListener("popstate", handleHistoryChange);
  }, []);

  useEffect(() => {
    if (!current || requestedVariant === current.key) return;
    selectVariant(current.key);
  }, [current, requestedVariant, selectVariant]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      const active = document.activeElement;
      if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement || active?.getAttribute("contenteditable") === "true") return;
      if (!current) return;
      const direction = event.key === "ArrowRight" ? 1 : -1;
      selectVariant(adjacentVariantKey(definition, current.key, direction));
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [current, definition, selectVariant]);

  if (!current) return <PrototypeProblem message="This prototype does not define any variants." />;
  const CurrentVariant = current.Component;
  return (
    <>
      <div className="prototype-marker" title={definition.question}>Prototype</div>
      <CurrentVariant />
      {definition.variants.length > 1 ? (
        <nav className="prototype-switcher" aria-label="Prototype variants">
          <button onClick={() => selectVariant(adjacentVariantKey(definition, current.key, -1))} type="button" aria-label="Previous variant">←</button>
          <span>{current.key} · {current.name}</span>
          <button onClick={() => selectVariant(adjacentVariantKey(definition, current.key, 1))} type="button" aria-label="Next variant">→</button>
        </nav>
      ) : null}
    </>
  );
}

function PrototypeProblem({ message }: { message: string }) {
  return <main className="prototype-index"><h1>Prototype cannot render</h1><p>{message}</p></main>;
}

function adjacentVariantKey(definition: PrototypeDefinition, currentKey: string, direction: number) {
  const index = definition.variants.findIndex((variant) => variant.key === currentKey);
  const next = (index + direction + definition.variants.length) % definition.variants.length;
  return definition.variants[next]!.key;
}

function variantFromUrl() {
  return new URLSearchParams(window.location.search).get("variant");
}
