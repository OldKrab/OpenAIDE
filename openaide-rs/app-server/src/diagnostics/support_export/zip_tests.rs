use std::collections::BTreeMap;
use std::fs;

use super::write_stored_zip;

#[test]
fn stored_zip_contains_manifest_and_payload_entries() {
    let directory = tempfile::TempDir::new().expect("temporary directory");
    let path = directory.path().join("bundle.zip");
    write_stored_zip(
        &path,
        BTreeMap::from([
            ("manifest.json".to_string(), b"{}\n".to_vec()),
            ("logs/app.jsonl".to_string(), b"{}\n".to_vec()),
        ]),
    )
    .expect("write zip");
    let bytes = fs::read(path).expect("read zip");

    assert_eq!(&bytes[..4], &0x04034b50_u32.to_le_bytes());
    assert_eq!(
        bytes
            .windows(4)
            .filter(|window| *window == 0x02014b50_u32.to_le_bytes())
            .count(),
        2
    );
    assert!(bytes
        .windows("manifest.json".len())
        .any(|window| window == b"manifest.json"));
}
