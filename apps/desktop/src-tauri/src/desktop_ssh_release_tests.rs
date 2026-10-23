use std::io::Read;

use super::*;

#[test]
fn a_bundled_binary_is_its_own_release() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("openaide-app-server");
    std::fs::write(&path, b"binary").unwrap();

    let source = local("0.11.0", path).unwrap();

    assert_eq!(source.release.version(), "0.11.0");
    let mut opened = Vec::new();
    source.open().unwrap().read_to_end(&mut opened).unwrap();
    assert_eq!(opened, b"binary");
}
