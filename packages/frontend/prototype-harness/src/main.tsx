import { useEffect } from "react";
import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "../../src/styles/tokens.css";
import "../../src/styles/app.css";
import "./prototype-harness.css";
import type { PrototypeDefinition } from "./prototypeApi";

type PrototypeModule = { default?: PrototypeDefinition };

const prototypeModules = import.meta.glob<PrototypeModule>("../../prototypes/*/prototype.tsx", { eager: true });
const prototypes = Object.entries(prototypeModules)
  .map(([modulePath, module]) => ({ slug: prototypeSlug(modulePath), definition: module.default }))
  .filter((entry): entry is { slug: string; definition: PrototypeDefinition } => (
    entry.slug !== undefined && validDefinition(entry.definition)
  ))
  .sort((left, right) => left.slug.localeCompare(right.slug));

const slug = requestedPrototypeSlug(window.location.pathname);
const selected = slug ? prototypes.find((entry) => entry.slug === slug) : undefined;
document.title = selected ? `${selected.definition.title} · Prototype` : "OpenAIDE Prototypes";

createRoot(document.getElementById("root")!).render(
  selected
    ? <PrototypeView definition={selected.definition} />
    : <PrototypeIndex missingSlug={slug} />,
);

function PrototypeView({ definition }: { definition: PrototypeDefinition }) {
  const requestedVariant = new URLSearchParams(window.location.search).get("variant");
  const fallbackKey = definition.defaultVariant ?? definition.variants[0]?.key;
  const current = definition.variants.find((variant) => variant.key === requestedVariant)
    ?? definition.variants.find((variant) => variant.key === fallbackKey)
    ?? definition.variants[0];

  useEffect(() => {
    if (!current || requestedVariant === current.key) return;
    replaceVariant(current.key);
  }, [current, requestedVariant]);

  useEffect(() => {
    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== "ArrowLeft" && event.key !== "ArrowRight") return;
      const active = document.activeElement;
      if (active instanceof HTMLInputElement || active instanceof HTMLTextAreaElement || active instanceof HTMLSelectElement || active?.getAttribute("contenteditable") === "true") return;
      const currentIndex = definition.variants.findIndex((variant) => variant.key === current?.key);
      const direction = event.key === "ArrowRight" ? 1 : -1;
      const nextIndex = (currentIndex + direction + definition.variants.length) % definition.variants.length;
      replaceVariant(definition.variants[nextIndex]!.key);
    };
    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [current?.key, definition.variants]);

  if (!current) return <PrototypeProblem message="This prototype does not define any variants." />;
  const CurrentVariant = current.Component;
  return (
    <>
      <div className="prototype-marker" title={definition.question}>Prototype</div>
      <CurrentVariant />
      {definition.variants.length > 1 ? (
        <nav className="prototype-switcher" aria-label="Prototype variants">
          <button onClick={() => cycleVariant(definition, current.key, -1)} type="button" aria-label="Previous variant">←</button>
          <span>{current.key} · {current.name}</span>
          <button onClick={() => cycleVariant(definition, current.key, 1)} type="button" aria-label="Next variant">→</button>
        </nav>
      ) : null}
    </>
  );
}

function PrototypeIndex({ missingSlug }: { missingSlug?: string }) {
  return (
    <main className="prototype-index">
      <header>
        <span>Prototype workbench</span>
        <h1>{missingSlug ? "Prototype not found" : "OpenAIDE prototypes"}</h1>
        <p>{missingSlug ? `No ignored prototype named “${missingSlug}” is loaded.` : "Local, disposable experiments available in this workspace."}</p>
      </header>
      {prototypes.length ? (
        <ul>{prototypes.map(({ slug, definition }) => (
          <li key={slug}><a href={`/prototype/${slug}/`}><strong>{definition.title}</strong><small>{definition.question}</small></a></li>
        ))}</ul>
      ) : <p className="prototype-empty">Create one with <code>npm run prototype:new -- example</code>.</p>}
    </main>
  );
}

function PrototypeProblem({ message }: { message: string }) {
  return <main className="prototype-index"><h1>Prototype cannot render</h1><p>{message}</p></main>;
}

function cycleVariant(definition: PrototypeDefinition, currentKey: string, direction: number) {
  const index = definition.variants.findIndex((variant) => variant.key === currentKey);
  const next = (index + direction + definition.variants.length) % definition.variants.length;
  replaceVariant(definition.variants[next]!.key);
}

function replaceVariant(key: string) {
  const url = new URL(window.location.href);
  url.searchParams.set("variant", key);
  window.history.replaceState(null, "", url);
  window.dispatchEvent(new PopStateEvent("popstate"));
  window.location.reload();
}

function requestedPrototypeSlug(pathname: string) {
  const match = /^\/prototype\/([^/]+)\/?$/.exec(pathname);
  return match?.[1] ? decodeURIComponent(match[1]) : undefined;
}

function prototypeSlug(modulePath: string) {
  return /\/prototypes\/([^/]+)\/prototype\.tsx$/.exec(modulePath)?.[1];
}

function validDefinition(value: PrototypeDefinition | undefined): value is PrototypeDefinition {
  return Boolean(value?.title && value.question && Array.isArray(value.variants));
}
