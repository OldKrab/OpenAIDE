import mermaid from "mermaid";
import {
  MERMAID_EDGE_LIMIT,
  MERMAID_OUTPUT_LIMIT_BYTES,
  MERMAID_RENDERER_VERSION,
  MERMAID_SOURCE_LIMIT,
  isMermaidRenderRequest,
  type MermaidRenderResponse,
  type MermaidTheme,
} from "./rendererProtocol";

const encoder = new TextEncoder();

window.addEventListener("message", (event) => {
  if (event.source !== window.parent || !isMermaidRenderRequest(event.data)) return;
  void renderDiagram(event.data.requestId, event.data.source, event.data.theme).then((result) => {
    const response: MermaidRenderResponse = {
      type: "openaide.mermaid.result",
      requestId: event.data.requestId,
      result,
    };
    window.parent.postMessage(response, "*");
  });
});

window.parent.postMessage({ type: "openaide.mermaid.ready", version: MERMAID_RENDERER_VERSION }, "*");

async function renderDiagram(requestId: string, source: string, theme: MermaidTheme): Promise<MermaidRenderResponse["result"]> {
  if (source.length > MERMAID_SOURCE_LIMIT) return { kind: "too_large" };
  try {
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      htmlLabels: false,
      suppressErrorRendering: true,
      maxTextSize: MERMAID_SOURCE_LIMIT,
      maxEdges: MERMAID_EDGE_LIMIT,
      theme: "base",
      fontFamily: theme.fontFamily,
      secure: [
        "secure",
        "securityLevel",
        "startOnLoad",
        "suppressErrorRendering",
        "maxTextSize",
        "maxEdges",
        "htmlLabels",
        "theme",
        "themeCSS",
        "themeVariables",
        "fontFamily",
        "altFontFamily",
      ],
      themeVariables: {
        background: theme.background,
        primaryColor: theme.panel,
        primaryTextColor: theme.text,
        primaryBorderColor: theme.border,
        lineColor: theme.muted,
        secondaryColor: theme.background,
        tertiaryColor: theme.panel,
        textColor: theme.text,
        mainBkg: theme.panel,
        nodeBorder: theme.border,
        clusterBkg: theme.background,
        clusterBorder: theme.border,
        edgeLabelBackground: theme.background,
      },
    });
    const parsed = await mermaid.parse(source, { suppressErrors: true });
    if (!parsed) return { kind: "invalid" };
    const rendered = await mermaid.render(`openaide-mermaid-${requestId}`, source);
    const sanitized = sanitizeStaticSvg(rendered.svg, theme);
    if (!sanitized) return { kind: "failed" };
    const outputBytes = encoder.encode(sanitized.svg).byteLength;
    if (outputBytes > MERMAID_OUTPUT_LIMIT_BYTES) return { kind: "too_large" };
    return { kind: "success", ...sanitized, outputBytes };
  } catch (error) {
    return classifiedFailure(error);
  } finally {
    document.querySelectorAll(`[id^="dopenaide-mermaid-${requestId}"], [id^="openaide-mermaid-${requestId}"]`)
      .forEach((node) => node.remove());
  }
}

function sanitizeStaticSvg(svg: string, theme: MermaidTheme): { svg: string; title?: string; description?: string } | undefined {
  const documentNode = new DOMParser().parseFromString(svg, "image/svg+xml");
  const root = documentNode.documentElement;
  if (root.tagName.toLowerCase() !== "svg" || documentNode.querySelector("parsererror")) return undefined;

  documentNode.querySelectorAll("script, foreignObject, iframe, object, embed, image").forEach((node) => node.remove());
  documentNode.querySelectorAll("a").forEach((link) => link.replaceWith(...Array.from(link.childNodes)));
  documentNode.querySelectorAll("*").forEach((element) => {
    for (const attribute of Array.from(element.attributes)) {
      const name = attribute.name.toLowerCase();
      if (name.startsWith("on") || ((name === "href" || name === "xlink:href") && !attribute.value.startsWith("#"))) {
        element.removeAttribute(attribute.name);
      } else if (name === "style") {
        element.setAttribute(attribute.name, stripExternalCssReferences(attribute.value));
      } else if (/url\((?!["']?#)/i.test(attribute.value)) {
        element.removeAttribute(attribute.name);
      }
    }
  });
  documentNode.querySelectorAll("style").forEach((style) => {
    style.textContent = stripExternalCssReferences(style.textContent ?? "");
  });
  if (theme.highContrast) root.append(highContrastStyles(documentNode, theme));
  root.setAttribute("xmlns", "http://www.w3.org/2000/svg");

  return {
    svg: new XMLSerializer().serializeToString(root),
    title: boundedText(root.querySelector(":scope > title")?.textContent, 200),
    description: boundedText(root.querySelector(":scope > desc")?.textContent, 1_000),
  };
}

function highContrastStyles(documentNode: Document, theme: MermaidTheme) {
  const style = documentNode.createElementNS("http://www.w3.org/2000/svg", "style");
  style.textContent = `
    text, tspan { fill: ${theme.text} !important; color: ${theme.text} !important; }
    rect, circle, ellipse, polygon { fill: ${theme.background} !important; stroke: ${theme.border} !important; }
    path, line, polyline { stroke: ${theme.border} !important; }
  `;
  return style;
}

function stripExternalCssReferences(css: string) {
  return css
    .replace(/@import[^;]+;?/gi, "")
    .replace(/url\((?!["']?#)[^)]+\)/gi, "none");
}

function boundedText(value: string | null | undefined, limit: number) {
  const text = value?.replace(/\s+/g, " ").trim();
  return text ? text.slice(0, limit) : undefined;
}

function classifiedFailure(error: unknown): MermaidRenderResponse["result"] {
  const message = error instanceof Error ? error.message : "";
  if (/maximum.*(?:text|edge)|too many edges/i.test(message)) return { kind: "too_large" };
  return { kind: "invalid" };
}
