//! What an App Shell may do with an App Server of another version that already
//! holds the SSH Computer's state root (ADR-0069): attach to an older one that
//! still speaks its protocol, replace an older one only with the user's
//! confirmation, and never replace a newer one.

use std::cmp::Ordering;

use openaide_app_server_protocol::snapshot::ProtocolVersion;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Running {
    /// Older, and it accepts this App Shell's protocol: attach and offer an update.
    OlderCompatible,
    /// Older, and this App Shell cannot use it: only a confirmed update helps.
    OlderIncompatible,
    /// Newer: replacing it would downgrade the computer.
    Newer,
    /// The versions cannot be ordered, so neither attaching nor replacing is safe.
    Unknown,
}

pub(crate) fn classify(
    shell_version: &str,
    shell_protocol: ProtocolVersion,
    running_version: &str,
    running_protocol: Option<&str>,
) -> Running {
    match compare(running_version, shell_version) {
        Some(Ordering::Greater) => Running::Newer,
        Some(Ordering::Less) => {
            let accepts = running_protocol
                .and_then(|text| text.parse::<ProtocolVersion>().ok())
                .is_some_and(|running| running.accepts_client(shell_protocol));
            if accepts {
                Running::OlderCompatible
            } else {
                Running::OlderIncompatible
            }
        }
        // An equal version cannot be the mismatch the keeper reported.
        Some(Ordering::Equal) | None => Running::Unknown,
    }
}

/// Orders release versions `major.minor.patch[-pre][+build]`. A pre-release is
/// older than its release; two different pre-releases of one release are not
/// ordered, because nothing here needs to choose between them.
pub(crate) fn compare(left: &str, right: &str) -> Option<Ordering> {
    let (left_core, left_pre) = parse(left)?;
    let (right_core, right_pre) = parse(right)?;
    match left_core.cmp(&right_core) {
        Ordering::Equal => match (left_pre, right_pre) {
            (None, None) => Some(Ordering::Equal),
            (None, Some(_)) => Some(Ordering::Greater),
            (Some(_), None) => Some(Ordering::Less),
            (Some(left), Some(right)) => (left == right).then_some(Ordering::Equal),
        },
        ordering => Some(ordering),
    }
}

fn parse(version: &str) -> Option<([u64; 3], Option<&str>)> {
    let version = version.split_once('+').map_or(version, |(core, _)| core);
    let (core, pre) = match version.split_once('-') {
        Some((core, pre)) => (core, Some(pre)),
        None => (version, None),
    };
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let parsed = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some((parsed, pre))
}

#[cfg(test)]
#[path = "version_tests.rs"]
mod tests;
