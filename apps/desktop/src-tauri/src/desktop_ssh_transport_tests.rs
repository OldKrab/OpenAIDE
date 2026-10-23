use super::*;

#[test]
fn ssh_diagnostics_become_classes() {
    let cases = [
        ("Host key verification failed.", "host_key_rejected"),
        (
            "someone@host: Permission denied (publickey,password).",
            "auth_failed",
        ),
        (
            "ssh: Could not resolve hostname nowhere: Name or service not known",
            "host_unknown",
        ),
        (
            "ssh: connect to host h port 22: Connection refused",
            "connection_refused",
        ),
        (
            "ssh: connect to host h port 22: Operation timed out",
            "unreachable",
        ),
        (
            "/home/someone/.ssh/config: line 3: Bad configuration option: foo",
            "ssh_config_invalid",
        ),
        (
            "Error: remote port forwarding failed for listen port 1",
            "forward_failed",
        ),
        ("something new", "ssh_failed"),
    ];
    for (stderr, class) in cases {
        assert_eq!(classify(stderr), class, "{stderr}");
    }
}

#[test]
fn a_destination_cannot_become_an_option_or_several_words() {
    assert!(is_destination("devbox"));
    assert!(is_destination("ssh://someone@build.example:2222"));
    assert!(!is_destination("-oProxyCommand=touch"));
    assert!(!is_destination("devbox other"));
    assert!(!is_destination("devbox\n"));
    assert!(!is_destination(""));
}
