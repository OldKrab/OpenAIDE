const browserConnection = {
  kind: "webProxy",
  endpointUrl: "/__openaide-app-server/probe",
};

const webRoutes = [
  { pattern: /^\/(?:new-task)?$/, surface: "task" },
  { pattern: /^\/archive\/?$/, surface: "task", archived: true },
  { pattern: /^\/settings\/?$/, surface: "settings" },
  { pattern: /^\/task\/([^/]+)\/?$/, surface: "task" },
  { pattern: /^\/task\/?$/, surface: "task" },
];

export function injectBootstrap(html, route, presentation = {}) {
  const attrs = [
    'data-shell="web"',
    'data-navigation-mode="project"',
    `data-surface="${route.surface}"`,
    route.taskId ? `data-task-id="${escapeAttribute(route.taskId)}"` : undefined,
    route.agentId ? `data-agent-id="${escapeAttribute(route.agentId)}"` : undefined,
    route.nativeSessionId ? `data-native-session-id="${escapeAttribute(route.nativeSessionId)}"` : undefined,
    route.archived ? 'data-archived="true"' : undefined,
    presentation.instanceLabel ? `data-instance-label="${escapeAttribute(presentation.instanceLabel)}"` : undefined,
    `data-app-server-connection="${escapeAttribute(JSON.stringify(browserConnection))}"`,
  ].filter(Boolean).join(" ");
  const titled = presentation.title ? injectTitle(html, presentation.title) : html;
  if (/<body([^>]*)>/i.test(html)) {
    return titled.replace(/<body([^>]*)>/i, `<body$1 ${attrs}>`);
  }
  return titled.replace(/<div id="root"><\/div>/i, `<body ${attrs}><div id="root"></div></body>`);
}

export function webRoute(pathname) {
  const nativeSessionMatch = /^\/session\/([^/]+)\/([^/]+)\/?$/.exec(pathname);
  if (nativeSessionMatch) {
    const agentId = decodedPathSegment(nativeSessionMatch[1]);
    const nativeSessionId = decodedPathSegment(nativeSessionMatch[2]);
    if (agentId === undefined || nativeSessionId === undefined) return undefined;
    return {
      agentId,
      archived: undefined,
      nativeSessionId,
      surface: "nativeSession",
      taskId: undefined,
    };
  }
  for (const route of webRoutes) {
    const match = route.pattern.exec(pathname);
    if (match) {
      return {
        archived: route.archived,
        surface: route.surface,
        taskId: match[1],
      };
    }
  }
  return undefined;
}

/** Maps streaming support routes without making the proxy infer subpaths ad hoc. */
export function appServerTransportRoute(method, pathname) {
  if (method === "POST" && pathname.endsWith("/upload/chunk")) {
    return { kind: "upload", appServerSuffix: "upload/chunk" };
  }
  if (method === "POST" && pathname.endsWith("/upload")) {
    return { kind: "upload", appServerSuffix: "upload" };
  }
  if (method === "GET" && pathname.endsWith("/download")) {
    return { kind: "download", appServerSuffix: "download" };
  }
  return undefined;
}

function decodedPathSegment(value) {
  try {
    return decodeURIComponent(value);
  } catch {
    return undefined;
  }
}

function escapeAttribute(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll('"', "&quot;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}

function injectTitle(html, title) {
  const escaped = escapeText(title);
  if (/<title>.*?<\/title>/is.test(html)) {
    return html.replace(/<title>.*?<\/title>/is, `<title>${escaped}</title>`);
  }
  if (/<\/head>/i.test(html)) {
    return html.replace(/<\/head>/i, `<title>${escaped}</title></head>`);
  }
  return html;
}

function escapeText(value) {
  return value
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;");
}
