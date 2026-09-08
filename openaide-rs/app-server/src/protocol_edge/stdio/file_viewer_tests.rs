use super::ProtocolEdgeStdioDispatcher;
use crate::protocol::model::{IsolationKind, TaskStatus};
use crate::storage::records::{TaskPreparationRecord, TaskRecord};
use crate::storage::Store;
use crate::storage_runtime::StateRoot;
use openaide_app_server_protocol::methods::{CLIENT_INITIALIZE, FILE_VIEWER_OPEN};
use serde_json::{json, Value};

fn init_request(id: &str, client_id: &str) -> String {
    json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": CLIENT_INITIALIZE,
        "params": {
            "clientInstanceId": client_id,
            "shell": { "kind": "web" },
            "requestedSurface": { "kind": "home" },
            "capabilities": {
                "protocol": ["permissionResponses", "questionResponses"]
            }
        },
        "meta": { "clientRequestId": "client-request-1" }
    })
    .to_string()
}

fn response(line: &str) -> Value {
    serde_json::from_str(line).expect("json response")
}

fn task_record(task_id: &str, workspace_root: String) -> TaskRecord {
    TaskRecord {
        task_id: task_id.to_string(),
        title: crate::storage::records::TaskTitleState::from_title(
            crate::storage::records::TaskTitle::new(
                "Task",
                crate::storage::records::TaskTitleSource::User,
            ),
        ),
        status: TaskStatus::Inactive,
        task_version: 1,
        message_history_version: 0,
        unread: false,
        pinned: false,
        attention: None,
        created_at: "2026-01-01T00:00:00.000Z".to_string(),
        updated_at: "2026-01-01T00:00:00.000Z".to_string(),
        last_activity: "2026-01-01T00:00:00.000Z".to_string(),
        permission_policy: Default::default(),
        composer_history: Default::default(),
        message_queue: Default::default(),
        agent_id: "codex".to_string(),
        agent_name: "Codex".to_string(),
        isolation: IsolationKind::Local,
        workspace_root,
        project_root: None,
        worktree_id: None,
        lifecycle: crate::storage::records::TaskLifecycle::Open,
        agent_session_id: None,
        active_turn_id: None,
        active_turn_started_at: None,
        tombstoned: false,
        revision: 1,
        config_options_catalog: None,
        native_session_data_freshness: Default::default(),
        native_session_reload_requirement: None,
        config_mutation: Default::default(),
        agent_commands_catalog: None,
        context_usage: None,
        current_plan: None,
        completed_plan_message_id: None,
        last_turn_usage: None,
        model_id: None,
        supports_image_input: false,
        preparation: TaskPreparationRecord::Ready,
    }
}

#[test]
fn file_viewer_open_returns_a_snapshot_handle_through_protocol() {
    let temp = tempfile::TempDir::new().expect("temp dir");
    let workspace = temp.path().join("workspace");
    std::fs::create_dir_all(&workspace).unwrap();
    std::fs::write(workspace.join("README.md"), "# Hello\n").unwrap();
    {
        let store = Store::open(temp.path().to_path_buf()).unwrap();
        store
            .write_task(&task_record(
                "task-file-viewer",
                workspace.to_string_lossy().to_string(),
            ))
            .unwrap();
    }
    let state_root = StateRoot::resolve(temp.path()).expect("state root");
    let mut dispatcher = ProtocolEdgeStdioDispatcher::new_for_test(state_root);
    dispatcher.handle_line(&init_request("1", "client-1"));

    let responses = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0",
            "id": "open",
            "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-file-viewer", "path": "README.md" }
        })
        .to_string(),
    );
    let snapshot = &response(&responses[0])["result"]["result"];

    assert_eq!(snapshot["kind"], "markdown");
    assert_eq!(snapshot["basename"], "README.md");
    assert_eq!(snapshot["text"], "# Hello\n");
    assert!(snapshot["handle"]
        .as_str()
        .unwrap()
        .starts_with("file-viewer-"));
}

#[test]
fn file_viewer_download_streams_current_bytes_without_preview_limits() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("сборка build.vsix");
    std::fs::write(&path, [0xff, 0x00]).unwrap();
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-download",
            temp.path().to_string_lossy().into_owned(),
        ))
        .unwrap();
    let mut dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    dispatcher.handle_line(&init_request("init", "client-download"));
    let replies = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0", "id": "open", "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-download", "path": "сборка build.vsix" }
        })
        .to_string(),
    );
    let reply = response(&replies[0]);
    let snapshot = &reply["result"]["result"];
    assert_eq!(snapshot["kind"], "binary");
    let handle = snapshot["handle"].as_str().unwrap();
    let current = vec![0xfd; 2 * 1024 * 1024];
    std::fs::write(&path, &current).unwrap();
    let (headers, body) = download_request(
        &dispatcher,
        &format!("fileViewerHandle={handle}&clientInstanceId=client-download"),
        Some("Bearer token"),
    );
    assert!(headers.starts_with("HTTP/1.1 200 OK"), "{headers}");
    assert!(
        headers.contains("filename*=UTF-8''%D1%81%D0%B1%D0%BE%D1%80%D0%BA%D0%B0%20build.vsix"),
        "{headers}"
    );
    assert_eq!(body, current);
}

fn download_request(
    dispatcher: &ProtocolEdgeStdioDispatcher,
    query: &str,
    authorization: Option<&str>,
) -> (String, Vec<u8>) {
    use crate::protocol_edge::local_http::{listener::handle_app_stream, LocalHttpAppHandler};
    use std::io::{Read, Write};
    use std::net::{TcpListener, TcpStream};
    let (shutdown, _) = std::sync::mpsc::channel();
    let handler = LocalHttpAppHandler::new(
        dispatcher.shared_gateway(),
        "token",
        "test-server",
        "replacement",
        shutdown,
    );
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let worker = std::thread::spawn(move || {
        let (mut socket, _) = listener.accept().unwrap();
        handle_app_stream(&mut socket, &handler).unwrap();
    });
    let mut socket = TcpStream::connect(address).unwrap();
    socket
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    let auth = authorization
        .map(|value| format!("Authorization: {value}\r\n"))
        .unwrap_or_default();
    write!(socket, "GET /download?{query} HTTP/1.1\r\n{auth}\r\n").unwrap();
    let mut bytes = Vec::new();
    socket.read_to_end(&mut bytes).unwrap();
    worker.join().unwrap();
    let end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .unwrap()
        + 4;
    (
        String::from_utf8(bytes[..end].to_vec()).unwrap(),
        bytes[end..].to_vec(),
    )
}

#[test]
fn file_viewer_download_checks_readiness_and_revalidates_each_transfer() {
    let temp = tempfile::TempDir::new().unwrap();
    let path = temp.path().join("README");
    std::fs::write(&path, "current text").unwrap();
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-download",
            temp.path().to_string_lossy().into_owned(),
        ))
        .unwrap();
    let mut dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    dispatcher.handle_line(&init_request("init", "client-download"));
    let replies = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0", "id": "open", "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-download", "path": "README" }
        })
        .to_string(),
    );
    let reply = response(&replies[0]);
    let handle = reply["result"]["result"]["handle"].as_str().unwrap();
    let query = format!("fileViewerHandle={handle}&clientInstanceId=client-download");
    let (headers, body) = download_request(
        &dispatcher,
        &format!("{query}&check=1"),
        Some("Bearer token"),
    );
    assert!(headers.starts_with("HTTP/1.1 204"), "{headers}");
    assert!(body.is_empty(), "readiness must not transfer file bytes");
    let (_, body) = download_request(&dispatcher, &query, Some("Bearer token"));
    assert_eq!(body, b"current text");
    std::fs::rename(&path, temp.path().join("moved")).unwrap();
    for suffix in ["", "&check=1"] {
        let (headers, _) = download_request(
            &dispatcher,
            &format!("{query}{suffix}"),
            Some("Bearer token"),
        );
        assert!(headers.starts_with("HTTP/1.1 404"), "{headers}");
    }
    std::fs::create_dir(&path).unwrap();
    let (headers, _) = download_request(&dispatcher, &query, Some("Bearer token"));
    assert!(headers.starts_with("HTTP/1.1 400"), "{headers}");
    let (headers, _) = download_request(&dispatcher, &query, None);
    assert!(headers.starts_with("HTTP/1.1 401"), "{headers}");
    let (headers, _) = download_request(&dispatcher, &query, Some("Bearer wrong"));
    assert!(headers.starts_with("HTTP/1.1 403"), "{headers}");
    dispatcher.handle_line(&init_request("other", "client-other"));
    let (headers, _) = download_request(
        &dispatcher,
        &query.replace("client-download", "client-other"),
        Some("Bearer token"),
    );
    assert!(headers.starts_with("HTTP/1.1 404"), "{headers}");
}

#[cfg(unix)]
#[test]
fn file_viewer_download_follows_outside_symlinks_and_expires_with_the_tab() {
    let temp = tempfile::TempDir::new().unwrap();
    let workspace = temp.path().join("workspace");
    std::fs::create_dir(&workspace).unwrap();
    let target = temp.path().join("actual.bin");
    let alias = temp.path().join("alias.vsix");
    std::fs::write(&target, [0xff, 0, 1]).unwrap();
    std::os::unix::fs::symlink(&target, &alias).unwrap();
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-download",
            workspace.to_string_lossy().into_owned(),
        ))
        .unwrap();
    let mut dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    dispatcher.handle_line(&init_request("init", "client-download"));
    let replies = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0", "id": "open", "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-download", "path": alias }
        })
        .to_string(),
    );
    let reply = response(&replies[0]);
    let handle = reply["result"]["result"]["handle"].as_str().unwrap();
    let query = format!("fileViewerHandle={handle}&clientInstanceId=client-download");
    let (headers, bytes) = download_request(&dispatcher, &query, Some("Bearer token"));
    assert!(headers.contains("filename*=UTF-8''alias.vsix"), "{headers}");
    assert_eq!(bytes, [0xff, 0, 1]);
    dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0", "id": "release", "method": "fileViewer/release",
            "params": { "handle": handle }
        })
        .to_string(),
    );
    let (headers, _) = download_request(&dispatcher, &query, Some("Bearer token"));
    assert!(headers.starts_with("HTTP/1.1 404"), "{headers}");
}

#[test]
fn file_viewer_previews_large_photos_but_downloads_the_original() {
    use base64::Engine;
    let mut random = 17_u32;
    let image = image::RgbImage::from_fn(2400, 2400, |_, _| {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        image::Rgb([random as u8, (random >> 8) as u8, (random >> 16) as u8])
    });
    let mut original = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut original, 100)
        .encode_image(&image)
        .unwrap();
    assert!(
        original.len() > 5 * 1024 * 1024,
        "fixture must exceed the old encoded-size limit"
    );
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(temp.path().join("photo.jpg"), &original).unwrap();
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-image",
            temp.path().to_string_lossy().into_owned(),
        ))
        .unwrap();
    let mut dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    dispatcher.handle_line(&init_request("init", "client-image"));
    let replies = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0", "id": "open", "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-image", "path": "photo.jpg" }
        })
        .to_string(),
    );
    let reply = response(&replies[0]);
    let snapshot = &reply["result"]["result"];
    assert_eq!(snapshot["kind"], "image");
    assert_eq!(snapshot["truncated"], true);
    let data = snapshot["preview"]["dataUrl"].as_str().unwrap();
    let preview_bytes = base64::engine::general_purpose::STANDARD
        .decode(data.split_once(',').unwrap().1)
        .unwrap();
    assert!(preview_bytes.len() <= 2 * 1024 * 1024);
    let decoded = image::load_from_memory(&preview_bytes).unwrap();
    assert!(decoded.width() <= 2048 && decoded.height() <= 2048);
    let handle = snapshot["handle"].as_str().unwrap();
    let (_, downloaded) = download_request(
        &dispatcher,
        &format!("fileViewerHandle={handle}&clientInstanceId=client-image"),
        Some("Bearer token"),
    );
    assert_eq!(downloaded, original);
}

#[test]
fn image_preview_admission_does_not_block_protocol_requests() {
    use crate::protocol_edge::local_http::LocalHttpAppHandler;
    use std::time::{Duration, Instant};
    let temp = tempfile::TempDir::new().unwrap();
    image::RgbImage::new(2, 2)
        .save(temp.path().join("photo.png"))
        .unwrap();
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-image-admission",
            temp.path().to_string_lossy().into_owned(),
        ))
        .unwrap();
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
        &init_request("init", "client-image-admission"),
    );
    let permit = crate::file_viewer::pause_image_previews_for_test();
    let logs = crate::logging::capture_test_logs();
    let image_handler = handler.clone();
    let image_thread = std::thread::spawn(move || {
        image_handler.handle(
            Some("Bearer token"),
            Some("image-client"),
            &json!({
                "jsonrpc": "2.0", "id": "image-admission-open", "method": FILE_VIEWER_OPEN,
                "params": { "taskId": "task-image-admission", "path": "photo.png" }
            })
            .to_string(),
        )
    });
    let deadline = Instant::now() + Duration::from_secs(5);
    let mut started = false;
    while Instant::now() < deadline {
        if logs.snapshot().iter().any(|line| {
            line["event"] == "file_viewer_open_started"
                && line["fields"]["request_id"] == "image-admission-open"
        }) {
            started = true;
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    let (sender, receiver) = std::sync::mpsc::channel();
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
    let heartbeat = receiver.recv_timeout(Duration::from_secs(1));
    // Always release and join before asserting, so a red test cannot strand other image tests.
    drop(permit);
    let image = image_thread.join().unwrap();
    heartbeat_thread.join().unwrap();
    assert!(started, "image request did not reach admission");
    assert!(
        heartbeat.is_ok(),
        "image conversion blocked unrelated protocol traffic"
    );
    assert_eq!(heartbeat.unwrap().status, 200);
    assert_eq!(
        response(&image.body)[0]["result"]["result"]["kind"],
        "image"
    );
}

#[test]
fn file_viewer_reduced_photo_applies_exif_orientation_without_upscaling() {
    use image::ImageEncoder;
    let pixels = image::RgbImage::from_fn(100, 300, |x, _| {
        if x < 50 {
            image::Rgb([255, 0, 0])
        } else {
            image::Rgb([0, 0, 255])
        }
    });
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 95);
    // TIFF orientation 6: rotate the stored pixels 90 degrees clockwise for display.
    encoder
        .set_exif_metadata(
            b"II\x2a\0\x08\0\0\0\x01\0\x12\x01\x03\0\x01\0\0\0\x06\0\0\0\0\0\0\0".to_vec(),
        )
        .unwrap();
    encoder.encode_image(&pixels).unwrap();
    bytes.resize(3 * 1024 * 1024, 0); // Valid JPEG with extra trailing bytes requires re-encoding.
    let snapshot = image_snapshot(&bytes, "rotated.jpg");
    let preview = image::load_from_memory(&image_preview_bytes(&snapshot))
        .unwrap()
        .to_rgb8();
    assert_eq!(preview.dimensions(), (300, 100));
    assert!(
        preview.get_pixel(150, 20)[0] > 240,
        "red half must be above the blue half"
    );
    assert!(preview.get_pixel(150, 80)[2] > 240);
    assert_eq!(snapshot["truncated"], true);
}

#[test]
fn file_viewer_reduced_png_preserves_transparency_and_bounds_payload() {
    let mut random = 73_u32;
    let pixels = image::RgbaImage::from_fn(1400, 1400, |_, _| {
        random ^= random << 13;
        random ^= random >> 17;
        random ^= random << 5;
        image::Rgba([random as u8, (random >> 8) as u8, (random >> 16) as u8, 64])
    });
    let mut original = std::io::Cursor::new(Vec::new());
    pixels
        .write_to(&mut original, image::ImageFormat::Png)
        .unwrap();
    let snapshot = image_snapshot(original.get_ref(), "transparent.png");
    let bytes = image_preview_bytes(&snapshot);
    assert!(bytes.len() <= 2 * 1024 * 1024);
    let preview = image::load_from_memory(&bytes).unwrap().to_rgba8();
    assert_eq!(
        preview.get_pixel(preview.width() / 2, preview.height() / 2)[3],
        64
    );
    assert_eq!(snapshot["preview"]["mediaType"], "image/png");
    assert_eq!(snapshot["truncated"], true);
}

#[test]
fn file_viewer_keeps_small_image_bytes_including_gif_animation() {
    let mut animated = Vec::new();
    {
        let mut encoder = image::codecs::gif::GifEncoder::new(&mut animated);
        for color in [[255, 0, 0, 255], [0, 0, 255, 255]] {
            encoder
                .encode_frame(image::Frame::new(image::RgbaImage::from_pixel(
                    4,
                    3,
                    image::Rgba(color),
                )))
                .unwrap();
        }
    }
    let gif = image_snapshot(&animated, "animated.gif");
    assert_eq!(image_preview_bytes(&gif), animated);
    assert_eq!(gif["truncated"], false);
    for format in [
        image::ImageFormat::Jpeg,
        image::ImageFormat::Png,
        image::ImageFormat::WebP,
    ] {
        let mut bytes = std::io::Cursor::new(Vec::new());
        image::RgbImage::new(4, 3)
            .write_to(&mut bytes, format)
            .unwrap();
        let snapshot = image_snapshot(bytes.get_ref(), "small-image");
        assert_eq!(image_preview_bytes(&snapshot), *bytes.get_ref());
        assert_eq!(snapshot["truncated"], false);
    }
}

#[test]
fn file_viewer_rejects_excessive_decoded_pixels_without_sending_the_image() {
    let mut bytes = std::io::Cursor::new(Vec::new());
    image::RgbImage::new(6000, 6000)
        .write_to(&mut bytes, image::ImageFormat::Png)
        .unwrap();
    let snapshot = image_snapshot(bytes.get_ref(), "oversized.png");
    assert_eq!(snapshot["kind"], "error");
    assert_eq!(snapshot["error"], "unsupported");
    assert!(snapshot.get("preview").is_none());
    assert!(
        snapshot["handle"].is_string(),
        "Download authority survives a preview rejection"
    );
}

fn image_snapshot(bytes: &[u8], name: &str) -> Value {
    let temp = tempfile::TempDir::new().unwrap();
    std::fs::write(temp.path().join(name), bytes).unwrap();
    Store::open(temp.path().to_path_buf())
        .unwrap()
        .write_task(&task_record(
            "task-image",
            temp.path().to_string_lossy().into_owned(),
        ))
        .unwrap();
    let mut dispatcher =
        ProtocolEdgeStdioDispatcher::new_for_test(StateRoot::resolve(temp.path()).unwrap());
    dispatcher.handle_line(&init_request("init", "client-image"));
    let replies = dispatcher.handle_line(
        &json!({
            "jsonrpc": "2.0", "id": "open", "method": FILE_VIEWER_OPEN,
            "params": { "taskId": "task-image", "path": name }
        })
        .to_string(),
    );
    response(&replies[0])["result"]["result"].clone()
}

fn image_preview_bytes(snapshot: &Value) -> Vec<u8> {
    use base64::Engine;
    assert_eq!(snapshot["kind"], "image", "{snapshot}");
    base64::engine::general_purpose::STANDARD
        .decode(
            snapshot["preview"]["dataUrl"]
                .as_str()
                .unwrap()
                .split_once(',')
                .unwrap()
                .1,
        )
        .unwrap()
}
