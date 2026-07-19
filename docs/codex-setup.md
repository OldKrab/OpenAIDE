# Set up Codex

OpenAIDE starts Codex through Node.js tools available to the current App Shell. If OpenAIDE cannot access those tools, install the current Node.js LTS release from the [official Node.js download page](https://nodejs.org/en/download), then return to OpenAIDE and choose **Check again**.

OpenAIDE does not automatically recheck when the installer page closes or the App Shell regains focus. If a recheck still cannot access the Node.js tools, use the reload action offered by the current App Shell, then check again. Node.js may already work in a terminal while remaining unavailable to an App Shell that started with an older environment.

If Node.js tools are available but Codex still cannot start, choose **Try again**. Agent Settings shows the product status and recovery actions; Support Diagnostics retains technical failure details without exposing raw command errors in New Task or Task Navigation.
