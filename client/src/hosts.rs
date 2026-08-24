//! Hosts-file management.
//!
//! Adds, updates, or removes an entry in a standard hosts file.
//!
//! # Format
//! Lines managed by this tool are bracketed with markers so they can be
//! reliably identified on subsequent updates:
//!
//! ```text
//! # BEGIN dnsless-homelab: myserver.home
//! 192.168.1.42 myserver.home
//! # END dnsless-homelab: myserver.home
//! ```

use std::{
    fs,
    io::{self, Write},
    path::Path,
};

const TAG: &str = "dnsless-homelab";

fn begin_marker(hostname: &str) -> String {
    format!("# BEGIN {TAG}: {hostname}")
}

fn end_marker(hostname: &str) -> String {
    format!("# END {TAG}: {hostname}")
}

/// Update (or insert) a hosts-file entry for `hostname` pointing to `ip`.
///
/// The function is atomic on Linux/macOS: it writes to a temporary file and
/// then renames it over the target path.  On Windows it writes in-place
/// because rename across drives is not always available.
pub fn update_hosts_entry(
    hosts_path: impl AsRef<Path>,
    hostname: &str,
    ip: &str,
) -> io::Result<()> {
    let path = hosts_path.as_ref();

    let existing = if path.exists() {
        fs::read_to_string(path)?
    } else {
        String::new()
    };

    let new_content = replace_or_append(&existing, hostname, ip);

    // Write atomically using a temp file in the same directory.
    let dir = path.parent().unwrap_or_else(|| Path::new("."));
    let tmp_path = dir.join(format!(".dnsless-{hostname}.tmp"));
    {
        let mut f = fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&tmp_path)?;
        f.write_all(new_content.as_bytes())?;
        f.flush()?;
    }
    fs::rename(&tmp_path, path)?;

    Ok(())
}

/// Produce the new hosts-file content by replacing an existing managed block
/// or appending a new one.
fn replace_or_append(original: &str, hostname: &str, ip: &str) -> String {
    let begin = begin_marker(hostname);
    let end = end_marker(hostname);

    let new_block = format!("{begin}\n{ip} {hostname}\n{end}\n");

    if let (Some(start), Some(stop)) = (original.find(&begin), original.find(&end)) {
        // Replace existing block (inclusive of the end marker + newline).
        let after_end = stop + end.len();
        // Consume the trailing newline if present.
        let tail_start = if original[after_end..].starts_with('\n') {
            after_end + 1
        } else {
            after_end
        };
        format!(
            "{}{new_block}{}",
            &original[..start],
            &original[tail_start..]
        )
    } else {
        // Append at the end; ensure there's a newline separator.
        if original.is_empty() || original.ends_with('\n') {
            format!("{original}{new_block}")
        } else {
            format!("{original}\n{new_block}")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    #[test]
    fn append_to_empty_file() {
        let result = replace_or_append("", "server.home", "10.0.0.1");
        assert!(result.contains("10.0.0.1 server.home"));
        assert!(result.contains("BEGIN dnsless-homelab: server.home"));
        assert!(result.contains("END dnsless-homelab: server.home"));
    }

    #[test]
    fn append_to_existing_content() {
        let existing = "127.0.0.1 localhost\n";
        let result = replace_or_append(existing, "server.home", "10.0.0.1");
        assert!(result.starts_with("127.0.0.1 localhost\n"));
        assert!(result.contains("10.0.0.1 server.home"));
    }

    #[test]
    fn update_existing_entry() {
        let existing = "127.0.0.1 localhost\n\
            # BEGIN dnsless-homelab: server.home\n\
            192.168.1.1 server.home\n\
            # END dnsless-homelab: server.home\n";
        let result = replace_or_append(existing, "server.home", "10.0.0.99");
        assert!(result.contains("10.0.0.99 server.home"));
        assert!(!result.contains("192.168.1.1 server.home"));
        // Original content preserved
        assert!(result.contains("127.0.0.1 localhost"));
    }

    #[test]
    fn idempotent_update() {
        let first = replace_or_append("", "server.home", "10.0.0.1");
        let second = replace_or_append(&first, "server.home", "10.0.0.1");
        assert_eq!(first, second);
    }

    #[test]
    fn write_to_file() {
        let tmp = NamedTempFile::new().unwrap();
        let path = tmp.path();
        update_hosts_entry(path, "box.home", "192.168.1.5").unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert!(content.contains("192.168.1.5 box.home"));

        // Update again with a new IP.
        update_hosts_entry(path, "box.home", "192.168.1.6").unwrap();
        let content2 = fs::read_to_string(path).unwrap();
        assert!(content2.contains("192.168.1.6 box.home"));
        assert!(!content2.contains("192.168.1.5 box.home"));
    }
}
