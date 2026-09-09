use std::sync::{Arc, Barrier};

#[test]
fn concurrent_replacements_of_distinct_files_never_share_temporary_bytes() {
    let root = tempfile::tempdir().unwrap();
    let barrier = Arc::new(Barrier::new(4));
    std::thread::scope(|scope| {
        let writers = (0..4)
            .map(|index| {
                let barrier = barrier.clone();
                let destination = root.path().join(format!("settings.{index}"));
                scope.spawn(move || {
                    let expected = vec![b'a' + index; 64 * 1024];
                    let mut results = Vec::new();
                    for _ in 0..12 {
                        barrier.wait();
                        let result = super::write_bytes(&destination, &expected);
                        barrier.wait();
                        results.push((result, std::fs::read(&destination)));
                    }
                    // Finish every rendezvous before reporting a failed write.
                    for (result, bytes) in results {
                        result.unwrap();
                        assert!(
                            bytes.unwrap() == expected,
                            "another destination's bytes leaked"
                        );
                    }
                })
            })
            .collect::<Vec<_>>();
        for writer in writers {
            writer.join().unwrap();
        }
    });
}
