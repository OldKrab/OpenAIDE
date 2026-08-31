import type { WebviewBootstrap } from "../../../packages/frontend/src/state/surfaceTypes";
import type { FrontendShell } from "../../../packages/frontend/src/services/frontendShell";
import { startFrontend } from "../../../packages/frontend/src/startFrontend";
import { createWebAppShell } from "../../../apps/web/frontend/webAppShell";

document.body.dataset.shell = "desktop";

const webShell = createWebAppShell();
const desktopPlatform = new URLSearchParams(window.location.search).get("desktop-platform") === "windows"
  ? "windows"
  : "macos";
const asDesktopBootstrap = (bootstrap: WebviewBootstrap): WebviewBootstrap => bootstrap.surface === "invalid"
  ? bootstrap
  : { ...bootstrap, shell: { kind: "desktop", navigationMode: "project" } };

const desktopShell: FrontendShell = {
  ...webShell,
  bootstrap: () => asDesktopBootstrap(webShell.bootstrap()),
  desktopWindow: {
    platform: desktopPlatform,
    close: () => undefined,
    minimize: () => undefined,
    startDragging: () => undefined,
    toggleMaximize: () => undefined,
  },
  navigation: {
    ...webShell.navigation,
    subscribe(listener) {
      return webShell.navigation.subscribe((bootstrap) => listener(asDesktopBootstrap(bootstrap)));
    },
  },
};

startFrontend(desktopShell);
