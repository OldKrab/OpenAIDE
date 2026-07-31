import { createRoot } from "react-dom/client";
import "@fontsource-variable/inter";
import "../../src/styles/tokens.css";
import "../../src/styles/app.css";
import "./prototype-harness.css";
import type { PrototypeDefinition } from "./prototypeApi";
import { PrototypeView } from "./PrototypeView";

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
