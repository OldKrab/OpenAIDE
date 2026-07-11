use super::*;
use agent_client_protocol::schema::EmbeddedResourceResource;
use serde_json::json;

#[test]
fn text_prompt_does_not_require_prompt_capabilities() {
    let blocks = build_prompt_content("hello".to_string(), Vec::new()).unwrap();

    assert_eq!(blocks.len(), 1);
    match &blocks[0] {
        ContentBlock::Text(text) => assert_eq!(text.text, "hello"),
        other => panic!("expected text block, got {other:?}"),
    }
}

#[test]
fn file_attachments_become_resource_links_without_prompt_capabilities() {
    let blocks = build_prompt_content(
        "Use attached context".to_string(),
        vec![
            attachment("main #1.rs", "/workspace/src/main #1.rs", None),
            attachment("windows.rs", "C:\\Users\\Ada\\file 50%.rs", None),
            attachment(
                "encoded.rs",
                "file:///workspace/src/already%20encoded.rs",
                None,
            ),
            attachment("literal-percent.rs", "/workspace/src/literal 50%.rs", None),
        ],
    )
    .unwrap();

    assert_eq!(blocks.len(), 5);
    assert_resource_link(
        &blocks[1],
        "main #1.rs",
        "file:///workspace/src/main%20%231.rs",
    );
    assert_resource_link(
        &blocks[2],
        "windows.rs",
        "file:///C%3A/Users/Ada/file%2050%25.rs",
    );
    assert_resource_link(
        &blocks[3],
        "encoded.rs",
        "file:///workspace/src/already%20encoded.rs",
    );
    assert_resource_link(
        &blocks[4],
        "literal-percent.rs",
        "file:///workspace/src/literal%2050%25.rs",
    );
}

#[test]
fn embedded_text_payload_requires_and_uses_embedded_context_capability() {
    let blocks = build_prompt_content_with_policy(
        "Use attached context".to_string(),
        vec![Attachment {
            kind: "text".to_string(),
            label: "scratch".to_string(),
            path: None,
            payload: Some(json!({"text": "notes", "mime": "text/plain"})),
        }],
        PromptContentPolicy::new(PromptContentCapabilities {
            embedded_context: true,
            ..PromptContentCapabilities::default()
        }),
    )
    .unwrap();

    match &blocks[1] {
        ContentBlock::Resource(resource) => match &resource.resource {
            EmbeddedResourceResource::TextResourceContents(text) => {
                assert_eq!(text.text, "notes");
                assert_eq!(text.mime_type.as_deref(), Some("text/plain"));
                assert_eq!(text.uri, "openaide://attachment/scratch");
            }
            other => panic!("expected text resource, got {other:?}"),
        },
        other => panic!("expected embedded resource, got {other:?}"),
    }
}

#[test]
fn image_payload_requires_and_uses_image_prompt_capability() {
    let blocks = build_prompt_content_with_policy(
        "Use image".to_string(),
        vec![Attachment {
            kind: "image".to_string(),
            label: "pasted.png".to_string(),
            path: None,
            payload: Some(json!({"data": "aW1hZ2U=", "mimeType": "image/png"})),
        }],
        PromptContentPolicy::new(PromptContentCapabilities {
            image: true,
            ..PromptContentCapabilities::default()
        }),
    )
    .unwrap();

    match &blocks[1] {
        ContentBlock::Image(image) => {
            assert_eq!(image.data, "aW1hZ2U=");
            assert_eq!(image.mime_type, "image/png");
            assert_eq!(image.uri, None);
        }
        other => panic!("expected image block, got {other:?}"),
    }
}

#[test]
fn attachment_only_image_omits_an_empty_text_block() {
    let blocks = build_prompt_content_with_policy(
        String::new(),
        vec![Attachment {
            kind: "image".to_string(),
            label: "pasted.png".to_string(),
            path: None,
            payload: Some(json!({"data": "aW1hZ2U=", "mimeType": "image/png"})),
        }],
        PromptContentPolicy::new(PromptContentCapabilities {
            image: true,
            ..PromptContentCapabilities::default()
        }),
    )
    .unwrap();

    assert_eq!(blocks.len(), 1);
    assert!(matches!(blocks[0], ContentBlock::Image(_)));
}

#[test]
fn payload_with_file_path_downgrades_to_resource_link_without_embedded_context() {
    let blocks = build_prompt_content(
        "Use file".to_string(),
        vec![Attachment {
            kind: "text".to_string(),
            label: "notes.txt".to_string(),
            path: Some("/workspace/notes.txt".to_string()),
            payload: Some(json!({"text": "notes", "mime": "text/plain"})),
        }],
    )
    .unwrap();

    assert_resource_link(&blocks[1], "notes.txt", "file:///workspace/notes.txt");
}

#[test]
fn unsupported_payload_without_link_is_blocked() {
    let error = build_prompt_content(
        "Use attached context".to_string(),
        vec![Attachment {
            kind: "text".to_string(),
            label: "scratch".to_string(),
            path: None,
            payload: Some(json!({"text": "notes"})),
        }],
    )
    .unwrap_err();

    assert!(error.to_string().contains("embedded context"));
}

#[test]
fn unsupported_attachment_is_not_silently_dropped() {
    let error = build_prompt_content(
        "Use attached context".to_string(),
        vec![attachment("remote", "https://example.com/file.rs", None)],
    )
    .unwrap_err();

    assert!(error.to_string().contains("absolute file path"));
}

fn attachment(label: &str, path: &str, payload: Option<serde_json::Value>) -> Attachment {
    Attachment {
        kind: "file".to_string(),
        label: label.to_string(),
        path: Some(path.to_string()),
        payload,
    }
}

fn assert_resource_link(block: &ContentBlock, name: &str, uri: &str) {
    match block {
        ContentBlock::ResourceLink(resource) => {
            assert_eq!(resource.name, name);
            assert_eq!(resource.uri, uri);
        }
        other => panic!("expected resource link, got {other:?}"),
    }
}
