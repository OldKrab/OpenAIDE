use tauri::http::{StatusCode, header};

use crate::desktop_renderer_protocol::{pdf_document_csp, respond};

fn header_value(
    response: &tauri::http::Response<std::borrow::Cow<'static, [u8]>>,
    name: header::HeaderName,
) -> String {
    response
        .headers()
        .get(name)
        .map(|value| value.to_str().unwrap().to_string())
        .unwrap_or_default()
}

#[test]
fn pdf_document_carries_its_own_policy_and_no_inline_script() {
    let response = respond("/pdf.html", false, || {
        unreachable!("the document does not load the script")
    });

    assert_eq!(response.status(), StatusCode::OK);
    let csp = header_value(&response, header::CONTENT_SECURITY_POLICY);
    assert!(csp.contains("connect-src 'none'"), "{csp}");
    assert!(csp.contains("worker-src blob:"), "{csp}");
    assert!(csp.contains("script-src openaide-renderer:;"), "{csp}");
    let body = String::from_utf8(response.body().to_vec()).unwrap();
    assert!(
        body.contains(r#"<script src="./pdf-renderer.js"></script>"#),
        "{body}"
    );
    assert_eq!(body.matches("<script").count(), 1, "{body}");
}

#[test]
fn windows_admits_the_script_by_its_webview_host() {
    let csp = pdf_document_csp(true);

    assert!(
        csp.contains("script-src http://openaide-renderer.localhost;"),
        "{csp}"
    );
}

#[test]
fn renderer_script_is_served_as_javascript() {
    let response = respond("/pdf-renderer.js", false, || {
        Some((b"/* renderer */".to_vec(), "text/javascript".to_string()))
    });

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        header_value(&response, header::CONTENT_TYPE),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(
        header_value(&response, header::X_CONTENT_TYPE_OPTIONS),
        "nosniff"
    );
    assert_eq!(response.body().as_ref(), b"/* renderer */");
}

#[test]
fn missing_renderer_is_not_replaced_by_the_app_document() {
    // Tauri's resolver answers an unknown asset with `index.html`.
    let fallback = respond("/pdf-renderer.js", false, || {
        Some((b"<!doctype html>".to_vec(), "text/html".to_string()))
    });
    let missing = respond("/pdf-renderer.js", false, || None);

    assert_eq!(fallback.status(), StatusCode::NOT_FOUND);
    assert_eq!(missing.status(), StatusCode::NOT_FOUND);
}

#[test]
fn only_renderer_assets_are_reachable() {
    for path in [
        "/",
        "/index.html",
        "/assets/index.js",
        "/pdf-renderer.js/../index.html",
    ] {
        let response = respond(path, false, || {
            unreachable!("{path} must not resolve an asset")
        });
        assert_eq!(response.status(), StatusCode::NOT_FOUND, "{path}");
    }
}
