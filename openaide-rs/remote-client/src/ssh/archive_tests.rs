use super::*;

fn archive(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut builder = tar::Builder::new(flate2::write::GzEncoder::new(
        Vec::new(),
        flate2::Compression::fast(),
    ));
    for (path, bytes) in entries {
        let mut header = tar::Header::new_gnu();
        header.set_size(bytes.len() as u64);
        header.set_mode(0o755);
        header.set_cksum();
        builder.append_data(&mut header, path, *bytes).unwrap();
    }
    builder.into_inner().unwrap().finish().unwrap()
}

fn sha256(bytes: &[u8]) -> String {
    copy_hashing(bytes, &mut std::io::sink()).unwrap()
}

#[test]
fn keeps_only_the_release_binary_from_the_archive() {
    let directory = tempfile::tempdir().unwrap();
    let cache = directory.path().join("0.11.0/openaide-app-server");
    let packed = archive(&[
        ("openaide-app-server/README.md", b"guide"),
        (ARCHIVE_ENTRY, b"binary"),
    ]);

    assert_eq!(
        extract_verified(packed.as_slice(), &cache, &sha256(b"other")),
        Err("checksum_mismatch")
    );
    assert!(!cache.exists());
    assert_eq!(
        std::fs::read_dir(cache.parent().unwrap()).unwrap().count(),
        0
    );

    extract_verified(packed.as_slice(), &cache, &sha256(b"binary")).unwrap();
    assert_eq!(std::fs::read(&cache).unwrap(), b"binary");
}

#[test]
fn an_archive_without_the_binary_is_refused() {
    let directory = tempfile::tempdir().unwrap();
    let packed = archive(&[("elsewhere/openaide-app-server", b"binary")]);

    assert_eq!(
        extract_verified(
            packed.as_slice(),
            &directory.path().join("x/openaide-app-server"),
            &sha256(b"binary")
        ),
        Err("archive_invalid")
    );
    assert_eq!(
        extract_verified(
            &b"not gzip"[..],
            &directory.path().join("y/openaide-app-server"),
            &sha256(b"")
        ),
        Err("archive_invalid")
    );
}
