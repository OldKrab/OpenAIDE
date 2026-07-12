import { existsSync } from "node:fs";
import { mkdir, rm, writeFile } from "node:fs/promises";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";
import { createServer } from "vite";

const repoRoot = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "..");
const prototypeRoot = path.join(repoRoot, "packages/frontend/prototypes");
const harnessConfig = path.join(repoRoot, "packages/frontend/prototype-harness/vite.config.ts");
const [command, slug] = process.argv.slice(2);

switch (command) {
  case "new":
    await createPrototype(requireSlug(slug));
    break;
  case "clean":
    await cleanPrototype(requireSlug(slug));
    break;
  case "serve":
    await servePrototypes(slug ? requireSlug(slug) : undefined);
    break;
  default:
    usage();
    process.exitCode = 2;
}

async function createPrototype(prototypeSlug) {
  const directory = prototypeDirectory(prototypeSlug);
  if (existsSync(directory)) throw new Error(`Prototype already exists: ${relative(directory)}`);
  await mkdir(directory, { recursive: true });
  await writeFile(path.join(directory, "prototype.tsx"), prototypeTemplate(prototypeSlug), "utf8");
  console.log(`Created ignored prototype: ${relative(directory)}`);
  console.log(`Start it with: npm run prototype:target -- ${prototypeSlug}`);
}

async function cleanPrototype(prototypeSlug) {
  const directory = prototypeDirectory(prototypeSlug);
  if (!existsSync(directory)) throw new Error(`Prototype does not exist: ${relative(directory)}`);
  await rm(directory, { recursive: true });
  console.log(`Deleted prototype: ${relative(directory)}`);
}

async function servePrototypes(prototypeSlug) {
  if (prototypeSlug && !existsSync(prototypeDirectory(prototypeSlug))) {
    throw new Error(`Prototype does not exist: ${relative(prototypeDirectory(prototypeSlug))}`);
  }
  await mkdir(prototypeRoot, { recursive: true });
  const server = await createServer({ configFile: harnessConfig });
  await server.listen();
  const route = prototypeSlug ? `/prototype/${prototypeSlug}/` : "/prototype/";
  if (process.env.OPENAIDE_WEB_PUBLIC_URL) {
    console.log(`Target prototype: ${new URL(route, ensureTrailingSlash(process.env.OPENAIDE_WEB_PUBLIC_URL))}`);
  } else {
    console.log(`Target path: ${route}`);
    console.log("Set OPENAIDE_WEB_PUBLIC_URL to print a complete browser URL.");
  }
  console.log("Edits under packages/frontend/prototypes update through Vite HMR.");
}

function requireSlug(value) {
  if (!value || !/^[a-z0-9][a-z0-9-]*$/.test(value)) {
    throw new Error("Prototype name must use lowercase letters, numbers, and hyphens.");
  }
  return value;
}

function prototypeDirectory(prototypeSlug) {
  return path.join(prototypeRoot, prototypeSlug);
}

function relative(value) {
  return path.relative(repoRoot, value);
}

function ensureTrailingSlash(value) {
  return value.endsWith("/") ? value : `${value}/`;
}

function usage() {
  console.error("Usage: node scripts/prototype.mjs <new|serve|clean> [prototype-name]");
}

function prototypeTemplate(prototypeSlug) {
  const title = prototypeSlug.split("-").map((part) => part[0].toUpperCase() + part.slice(1)).join(" ");
  return `import { definePrototype, PrototypeCanvas } from "../../prototype-harness/src/prototypeApi";

function VariantA() {
  return <PrototypeCanvas>{/* Import and render production components here. */}</PrototypeCanvas>;
}

function VariantB() {
  return <PrototypeCanvas>{/* Keep variants structurally different. */}</PrototypeCanvas>;
}

function VariantC() {
  return <PrototypeCanvas>{/* Remove variants that do not answer the question. */}</PrototypeCanvas>;
}

export default definePrototype({
  title: "${title}",
  question: "What should this prototype help us decide?",
  variants: [
    { key: "A", name: "First direction", Component: VariantA },
    { key: "B", name: "Second direction", Component: VariantB },
    { key: "C", name: "Third direction", Component: VariantC },
  ],
});
`;
}
