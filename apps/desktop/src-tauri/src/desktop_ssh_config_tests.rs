use super::*;

#[test]
fn lists_concrete_aliases_including_included_files() {
    let home = tempfile::tempdir().unwrap();
    let ssh = home.path().join(".ssh");
    std::fs::create_dir_all(ssh.join("config.d")).unwrap();
    std::fs::write(
        ssh.join("config"),
        "# personal\nInclude config.d/*.conf\nHost devbox build-1 *.internal !bad\n  HostName 10.0.0.2\nHost=\"quoted\" # note\nMatch host x\n",
    )
    .unwrap();
    std::fs::write(ssh.join("config.d/vm.conf"), "Host vm\n").unwrap();
    std::fs::write(ssh.join("config.d/ignored.txt"), "Host ignored\n").unwrap();

    assert_eq!(host_aliases(&ssh), ["build-1", "devbox", "quoted", "vm"]);
}

#[test]
fn includes_that_loop_or_are_missing_end_quietly() {
    let home = tempfile::tempdir().unwrap();
    let ssh = home.path().join(".ssh");
    std::fs::create_dir_all(&ssh).unwrap();
    std::fs::write(
        ssh.join("config"),
        "Include config\nInclude missing\nHost a\n",
    )
    .unwrap();

    assert_eq!(host_aliases(&ssh), ["a"]);
    assert!(host_aliases(&home.path().join("absent")).is_empty());
}
