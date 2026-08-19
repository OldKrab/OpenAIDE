use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::Path;

/// Writes a store-only ZIP so sensitive inputs are never handed to an external archiver.
pub(super) fn write_stored_zip(
    path: &Path,
    entries: BTreeMap<String, Vec<u8>>,
) -> std::io::Result<()> {
    let mut file = fs::File::create(path)?;
    let mut central = Vec::new();
    let mut offset = 0_u32;
    let mut entry_count = 0_u16;
    for (name, bytes) in entries {
        entry_count = entry_count.saturating_add(1);
        let name_bytes = name.as_bytes();
        let crc = crc32(&bytes);
        let size = u32::try_from(bytes.len()).unwrap_or(u32::MAX);
        let mut local = Vec::new();
        local.extend_from_slice(&0x04034b50_u32.to_le_bytes());
        local.extend_from_slice(&20_u16.to_le_bytes());
        local.extend_from_slice(&0_u16.to_le_bytes());
        local.extend_from_slice(&0_u16.to_le_bytes());
        local.extend_from_slice(&0_u16.to_le_bytes());
        local.extend_from_slice(&0x21_u16.to_le_bytes());
        local.extend_from_slice(&crc.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(&size.to_le_bytes());
        local.extend_from_slice(
            &u16::try_from(name_bytes.len())
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        local.extend_from_slice(&0_u16.to_le_bytes());
        file.write_all(&local)?;
        file.write_all(name_bytes)?;
        file.write_all(&bytes)?;

        let mut header = Vec::new();
        header.extend_from_slice(&0x02014b50_u32.to_le_bytes());
        header.extend_from_slice(&20_u16.to_le_bytes());
        header.extend_from_slice(&20_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0x21_u16.to_le_bytes());
        header.extend_from_slice(&crc.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(&size.to_le_bytes());
        header.extend_from_slice(
            &u16::try_from(name_bytes.len())
                .unwrap_or(u16::MAX)
                .to_le_bytes(),
        );
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0_u16.to_le_bytes());
        header.extend_from_slice(&0_u32.to_le_bytes());
        header.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(&header);
        central.extend_from_slice(name_bytes);
        offset = offset.saturating_add(
            u32::try_from(local.len() + name_bytes.len() + bytes.len()).unwrap_or(u32::MAX),
        );
    }

    let central_offset = offset;
    file.write_all(&central)?;
    let mut end = Vec::new();
    end.extend_from_slice(&0x06054b50_u32.to_le_bytes());
    end.extend_from_slice(&0_u16.to_le_bytes());
    end.extend_from_slice(&0_u16.to_le_bytes());
    end.extend_from_slice(&entry_count.to_le_bytes());
    end.extend_from_slice(&entry_count.to_le_bytes());
    end.extend_from_slice(
        &u32::try_from(central.len())
            .unwrap_or(u32::MAX)
            .to_le_bytes(),
    );
    end.extend_from_slice(&central_offset.to_le_bytes());
    end.extend_from_slice(&0_u16.to_le_bytes());
    file.write_all(&end)?;
    file.flush()
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = 0xffff_ffff_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            crc = if crc & 1 == 1 {
                (crc >> 1) ^ 0xedb8_8320
            } else {
                crc >> 1
            };
        }
    }
    !crc
}

#[cfg(test)]
#[path = "zip_tests.rs"]
mod tests;
