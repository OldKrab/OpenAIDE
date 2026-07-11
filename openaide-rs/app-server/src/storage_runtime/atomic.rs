use std::fs::File;
use std::io::Write;
use std::path::Path;

pub fn write_bytes(path: &Path, bytes: &[u8]) -> Result<(), std::io::Error> {
    let parent = path
        .parent()
        .ok_or_else(|| std::io::Error::other("path has no parent"))?;
    std::fs::create_dir_all(parent)?;

    let tmp = path.with_extension("tmp");
    {
        let mut file = File::create(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all().ok();
    }
    std::fs::rename(tmp, path)?;
    Ok(())
}
