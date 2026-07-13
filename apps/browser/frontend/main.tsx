import { startFrontend } from "../../../packages/frontend/src/startFrontend";
import { createBrowserShell } from "./browserShell";

// The browser entrypoint is the sole composition root that chooses a concrete App Shell.
startFrontend(createBrowserShell());
