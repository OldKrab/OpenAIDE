//! Serves sandboxed renderer documents from their own origin (ADR-0061).
//!
//! The shared Frontend builds its PDF sandbox from `srcdoc` on other App Shells, but a
//! `srcdoc` frame enforces its parent's CSP too, and the Desktop page's CSP forbids the
//! renderer's script and parsing worker. A frame navigated to this scheme keeps only the
//! policy below. The frame stays `sandbox="allow-scripts"`: an opaque origin with no
//! Tauri IPC, receiving document bytes and returning scalar events by `postMessage`.
//!
//! TODO: serve the Mermaid renderer here too. Diagrams also build a `srcdoc` sandbox, so the
//! Desktop page's CSP blocks them the same way, and Desktop does not package that renderer.

use std::borrow::Cow;

use tauri::http::{Response, StatusCode, header};

pub(crate) const SCHEME: &str = "openaide-renderer";

const PDF_DOCUMENT: &str = "/pdf.html";
const PDF_SCRIPT: &str = "/pdf-renderer.js";

/// Static on purpose: the script is admitted by its scheme rather than by a nonce, so the
/// document has no inline script at all. Keep the other directives in step with
/// `buildPdfViewerDocument` in `packages/frontend/src/pdf/viewerFrame.ts`.
const PDF_DOCUMENT_HTML: &str = r#"<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <title>OpenAIDE PDF Preview</title>
  </head>
  <body>
    <script src="./pdf-renderer.js"></script>
  </body>
</html>
"#;

/// The script's origin as a CSP source. WebView2 exposes a custom scheme as an HTTP host;
/// WebKit keeps the scheme.
fn script_source(windows: bool) -> String {
    if windows {
        format!("http://{SCHEME}.localhost")
    } else {
        format!("{SCHEME}:")
    }
}

pub(crate) fn pdf_document_csp(windows: bool) -> String {
    format!(
        "default-src 'none'; connect-src 'none'; font-src data:; img-src data: blob:; style-src 'unsafe-inline'; worker-src blob:; script-src {}; base-uri 'none'; form-action 'none'",
        script_source(windows)
    )
}

/// Answers one request. `script` loads the packaged renderer as `(bytes, mime type)`.
pub(crate) fn respond(
    path: &str,
    windows: bool,
    script: impl FnOnce() -> Option<(Vec<u8>, String)>,
) -> Response<Cow<'static, [u8]>> {
    match path {
        PDF_DOCUMENT => {
            desktop_log!("desktop_renderer_served asset=pdf_document");
            Response::builder()
                .header(header::CONTENT_TYPE, "text/html; charset=utf-8")
                .header(header::CONTENT_SECURITY_POLICY, pdf_document_csp(windows))
                .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                .header(header::CACHE_CONTROL, "no-store")
                .body(Cow::Borrowed(PDF_DOCUMENT_HTML.as_bytes()))
                .expect("static renderer document response")
        }
        // The asset resolver answers a missing file with `index.html`, so only script counts.
        PDF_SCRIPT => match script().filter(|(_, mime)| mime.contains("javascript")) {
            Some((bytes, _)) => {
                desktop_log!(
                    "desktop_renderer_served asset=pdf_script bytes={}",
                    bytes.len()
                );
                Response::builder()
                    .header(header::CONTENT_TYPE, "text/javascript; charset=utf-8")
                    .header(header::X_CONTENT_TYPE_OPTIONS, "nosniff")
                    .header(header::CACHE_CONTROL, "no-store")
                    .body(Cow::Owned(bytes))
                    .expect("renderer script response")
            }
            None => {
                desktop_log!(
                    "desktop_renderer_request_failed asset=pdf_script error_kind=asset_missing"
                );
                not_found()
            }
        },
        _ => {
            desktop_log!("desktop_renderer_request_failed asset=unknown error_kind=not_found");
            not_found()
        }
    }
}

/// Registered on the Tauri builder for [`SCHEME`].
pub(crate) fn handle<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    path: &str,
) -> Response<Cow<'static, [u8]>> {
    respond(path, cfg!(windows), || {
        app.asset_resolver()
            .get(PDF_SCRIPT.to_string())
            .map(|asset| (asset.bytes, asset.mime_type))
    })
}

fn not_found() -> Response<Cow<'static, [u8]>> {
    Response::builder()
        .status(StatusCode::NOT_FOUND)
        .header(header::CACHE_CONTROL, "no-store")
        .body(Cow::Borrowed(&[][..]))
        .expect("static not-found response")
}

#[cfg(test)]
#[path = "desktop_renderer_protocol_tests.rs"]
mod tests;
