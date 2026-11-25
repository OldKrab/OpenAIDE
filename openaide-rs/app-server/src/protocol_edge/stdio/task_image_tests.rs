//! `task/image` through the local HTTP app handler (ADR-0075, ADR-0084).

use base64::Engine;
use serde_json::{json, Value};

use super::file_viewer_tests::{init_request, response, task_record};
use super::ProtocolEdgeStdioDispatcher;
use crate::protocol_edge::local_http::LocalHttpAppHandler;
use crate::storage::Store;
use crate::storage_runtime::StateRoot;

struct Fixture {
    _temp: tempfile::TempDir,
    handler: LocalHttpAppHandler,
    image_id: String,
    original: Vec<u8>,
}

/// An opaque screenshot-sized PNG with an alpha channel no pixel uses, stored as a
/// Chat image of `task-images`. Noise keeps it large, like a real screenshot.
fn fixture(width: u32, height: u32) -> Fixture {
    let temp = tempfile::TempDir::new().unwrap();
    // The store is closed again before the dispatcher opens the same state root.
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-images",
            temp.path().to_string_lossy().into_owned(),
        ))
        .unwrap();
    let mut seed = 0x9e37_79b9_u32;
    let pixels = image::RgbaImage::from_fn(width, height, |_, _| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let [r, g, b, _] = seed.to_le_bytes();
        image::Rgba([r, g, b, u8::MAX])
    });
    let mut original = Vec::new();
    pixels
        .write_to(
            &mut std::io::Cursor::new(&mut original),
            image::ImageFormat::Png,
        )
        .unwrap();
    let task_dir = temp.path().join("task-store-v1/tasks/task-images");
    let image_id = crate::storage::chat_images::store(&task_dir, &original).unwrap();
    let dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    let (shutdown, _) = std::sync::mpsc::channel();
    let handler = LocalHttpAppHandler::new(
        dispatcher.shared_gateway(),
        "token",
        "server",
        "replacement",
        shutdown,
    );
    handler.handle(
        Some("Bearer token"),
        Some("image-client"),
        &init_request("init", "client-images"),
    );
    Fixture {
        _temp: temp,
        handler,
        image_id,
        original,
    }
}

fn request_image(
    handler: &LocalHttpAppHandler,
    id: &str,
    image_id: &str,
    variant: Option<&str>,
) -> Value {
    let mut params = json!({ "taskId": "task-images", "imageId": image_id });
    if let Some(variant) = variant {
        params["variant"] = json!(variant);
    }
    let reply = handler.handle(
        Some("Bearer token"),
        Some("image-client"),
        &json!({ "jsonrpc": "2.0", "id": id, "method": "task/image", "params": params })
            .to_string(),
    );
    response(&reply.body)[0]["result"]["result"].clone()
}

fn decode(result: &Value) -> Vec<u8> {
    base64::engine::general_purpose::STANDARD
        .decode(result["data"].as_str().expect("image data"))
        .unwrap()
}

#[test]
fn a_chat_row_gets_a_smaller_jpeg_and_the_full_view_gets_the_stored_bytes() {
    let fixture = fixture(1400, 900);
    let logs = crate::logging::capture_test_logs();

    let preview = request_image(
        &fixture.handler,
        "preview-1",
        &fixture.image_id,
        Some("preview"),
    );
    assert_eq!(preview["mediaType"], "image/jpeg", "{preview}");
    let preview_bytes = decode(&preview);
    assert!(preview_bytes.len() < fixture.original.len() / 4);
    let shown = image::load_from_memory(&preview_bytes).unwrap();
    assert!(shown.width() <= 960 && shown.height() <= 960);
    // The preview keeps the image's shape, so the reserved footprint still fits it.
    let shape = f64::from(shown.width()) / f64::from(shown.height());
    assert!((shape - 1400.0 / 900.0).abs() < 0.01, "{shape}");

    // An older client sends no variant and gets the stored bytes, with no media type.
    let original = request_image(&fixture.handler, "original", &fixture.image_id, None);
    assert_eq!(original.get("mediaType"), None);
    assert_eq!(decode(&original), fixture.original);

    // A second preview comes from the cache, without decoding again.
    let again = request_image(
        &fixture.handler,
        "preview-2",
        &fixture.image_id,
        Some("preview"),
    );
    assert_eq!(decode(&again), preview_bytes);
    let served: Vec<Value> = logs
        .snapshot()
        .into_iter()
        .filter(|line| line["event"] == "task_image_served")
        .map(|line| line["fields"].clone())
        .collect();
    assert_eq!(served.len(), 3, "{served:?}");
    assert_eq!(served[0]["variant"], "preview");
    assert_eq!(served[0]["reduced"], true);
    assert_eq!(served[0]["cache_hit"], false);
    assert_eq!(served[0]["source_bytes"], fixture.original.len());
    assert_eq!(served[1]["variant"], "original");
    assert_eq!(served[1]["reduced"], false);
    assert_eq!(served[2]["cache_hit"], true);
}

#[test]
fn a_small_image_is_its_own_preview() {
    let fixture = fixture(40, 30);
    let preview = request_image(
        &fixture.handler,
        "small",
        &fixture.image_id,
        Some("preview"),
    );
    assert_eq!(preview.get("mediaType"), None, "{preview}");
    assert_eq!(decode(&preview), fixture.original);
}

#[test]
fn making_a_preview_does_not_block_protocol_requests() {
    let fixture = fixture(40, 30);
    let permit = crate::file_viewer::pause_image_previews_for_test();
    let logs = crate::logging::capture_test_logs();
    let image_handler = fixture.handler.clone();
    let image_id = fixture.image_id.clone();
    let image_thread = std::thread::spawn(move || {
        request_image(&image_handler, "held-preview", &image_id, Some("preview"))
    });
    crate::test_sync::wait_until("the preview request to pass its access check", || {
        logs.snapshot().iter().any(|line| {
            line["event"] == "task_image_started" && line["fields"]["variant"] == "preview"
        })
    });
    let (sender, receiver) = std::sync::mpsc::channel();
    let handler = fixture.handler.clone();
    let heartbeat_thread = std::thread::spawn(move || {
        let response = handler.handle(
            Some("Bearer token"),
            Some("image-client"),
            &json!({
                "jsonrpc": "2.0", "id": "heartbeat", "method": "client/heartbeat", "params": {}
            })
            .to_string(),
        );
        sender.send(response).unwrap();
    });
    let heartbeat = receiver.recv_timeout(crate::test_sync::WATCHDOG);
    // Always release and join before asserting, so a red test cannot strand other image tests.
    drop(permit);
    let preview = image_thread.join().unwrap();
    heartbeat_thread.join().unwrap();
    assert!(
        heartbeat.is_ok(),
        "making a preview blocked unrelated protocol traffic"
    );
    assert_eq!(heartbeat.unwrap().status, 200);
    assert_eq!(decode(&preview), fixture.original);
}
