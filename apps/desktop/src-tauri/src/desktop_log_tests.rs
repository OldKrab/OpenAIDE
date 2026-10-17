use super::{FILE_COUNT, Sink, numbered};

fn scratch() -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "openaide-desktop-log-test-{}",
        uuid::Uuid::new_v4()
    ))
}

#[test]
fn appends_timestamped_lines_across_reopen() {
    let root = scratch();
    let path = root.join("openaide-desktop.log");
    Sink::open(path.clone(), 1024)
        .unwrap()
        .append("first event=a");
    Sink::open(path.clone(), 1024)
        .unwrap()
        .append("second event=b");

    let text = std::fs::read_to_string(&path).unwrap();
    let lines: Vec<_> = text.lines().collect();
    assert_eq!(lines.len(), 2);
    let (timestamp, line) = lines[1].split_once(' ').unwrap();
    assert!(timestamp.parse::<u128>().unwrap() > 0);
    assert_eq!(line, "second event=b");
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn rotates_into_a_bounded_set_of_files() {
    let root = scratch();
    let path = root.join("openaide-desktop.log");
    let mut sink = Sink::open(path.clone(), 64).unwrap();
    for index in 0..20 {
        sink.append(&format!("event_{index:02} padding=xxxxxxxxxxxxxxxxxxxx"));
    }

    assert!(std::fs::read_to_string(&path).unwrap().contains("event_19"));
    assert!(numbered(&path, FILE_COUNT - 1).exists());
    assert!(!numbered(&path, FILE_COUNT).exists());
    for index in 0..FILE_COUNT {
        let file = if index == 0 {
            path.clone()
        } else {
            numbered(&path, index)
        };
        assert!(std::fs::metadata(file).unwrap().len() <= 64);
    }
    let _ = std::fs::remove_dir_all(root);
}
