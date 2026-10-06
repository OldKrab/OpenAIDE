# Sandbox PDF Preview Rendering

Status: accepted

The File Viewer previews PDF files in Web. App Server identifies a PDF by its `%PDF-` signature and reports the `pdf` snapshot kind without carrying bytes. Frontend then reads the current file through an App Shell capability under the same viewer handle authority as Download, bounded to 64 MiB, and transfers the bytes into a visible sandboxed renderer running the official pdf.js viewer. The renderer has scripts enabled, an opaque origin, and no network access; it owns scrolling, page virtualization, zoom, and the selectable text layer. The parent accepts only validated scalar events from it (ready, page count, current page, scale, classified failure) and renders the page position and zoom controls in the shared File Viewer chrome.

## Considered Options

Rasterizing pages in App Server and reusing the bounded image preview was rejected because pages would be images: no text selection, no in-page find, and zoom limited to the preview resolution. Embedding the browser's built-in PDF viewer was rejected because it is absent from the Linux Desktop webview and cannot be themed or bounded. Running pdf.js in the main document was rejected because it parses untrusted bytes inside the application's origin.

## Consequences

The PDF bytes are ephemeral presentation input, not snapshot state: Refresh rereads them, and nothing is cached across File Tabs. The renderer bundle carries pdf.js's legacy build and a classic inline parsing worker, the forms that start inside the sandbox on supported browsers. A shell without the byte capability keeps a fallback, so Desktop shows "PDF preview unavailable" until it supplies one, and VS Code does not package the renderer. The sandbox cannot fetch pdf.js decoder assets, so JPEG 2000 images and some CJK fonts degrade until those assets are packaged into the renderer bundle. Encrypted, damaged, and oversized documents show a classified failure and keep Download. External links inside a document are inert; in-document destinations navigate. File Quotes from PDF text are not offered.
