use std::env;
use std::path::PathBuf;

pub(super) fn normalized_session_cwd(cwd: &str) -> PathBuf {
    let candidate = if cwd.trim().is_empty() {
        env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
    } else {
        PathBuf::from(cwd)
    };

    if candidate.is_absolute() {
        candidate
    } else {
        env::current_dir()
            .unwrap_or_else(|_| PathBuf::from("/"))
            .join(candidate)
    }
}
