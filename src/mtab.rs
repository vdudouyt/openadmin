//! `/etc/mtab` parsing — mirrors qhostman's `parseMTab()`
//! (`gui/mtab_parser.cpp`): the set of currently mounted mount points, with
//! `\040` unescaped back to a space. Drives the MNT column.

use std::collections::HashSet;

pub fn parse_mtab() -> HashSet<String> {
    parse(&std::fs::read_to_string("/etc/mtab").unwrap_or_default())
}

fn parse(data: &str) -> HashSet<String> {
    data.lines()
        .filter_map(|line| {
            // qhostman indexes components[1] unchecked; a short line here is
            // simply skipped.
            let mut parts = line.split_whitespace();
            let _dev = parts.next()?;
            let mount_point = parts.next()?;
            Some(mount_point.replace("\\040", " "))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_mount_points_and_unescapes_spaces() {
        let mtab = parse(
            "/dev/vda1 / ext4 rw 0 0\n\
             web-01:/ /net/web-01 fuse.sshfs rw 0 0\n\
             nas:/ /net/my\\040share fuse.sshfs rw 0 0\n",
        );
        assert!(mtab.contains("/"));
        assert!(mtab.contains("/net/web-01"));
        assert!(mtab.contains("/net/my share"));
    }

    #[test]
    fn short_lines_are_skipped_not_panicked_on() {
        let mtab = parse("garbage\n\n/dev/vda1 / ext4 rw 0 0\n");
        assert_eq!(mtab.len(), 1);
    }
}
