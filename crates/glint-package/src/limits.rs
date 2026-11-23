//! Resource budgets applied while reading an untrusted archive.

/// Resource budgets applied while reading an untrusted `.glint` archive.
///
/// Defaults follow the SPK-007 recommendations in
/// `docs/research/spikes/package-security.md`. Every limit is enforced on the
/// values the archive *declares* before anything is extracted, and again on
/// the bytes actually produced while extracting.
///
/// Construct with struct-update syntax:
///
/// ```
/// let limits = glint_package::PackageLimits {
///     max_entries: 500,
///     ..glint_package::PackageLimits::default()
/// };
/// assert_eq!(limits.max_ratio, 100);
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackageLimits {
    /// Maximum size of the `.glint` file, checked before parsing.
    pub max_archive_bytes: u64,
    /// Maximum number of central-directory entries (files and directories).
    /// Checked against the end-of-central-directory record before the
    /// directory is parsed.
    pub max_entries: usize,
    /// Maximum sum of uncompressed file sizes (declared and actual).
    pub max_total_uncompressed: u64,
    /// Maximum uncompressed size of one file (declared and actual).
    pub max_file_uncompressed: u64,
    /// Maximum uncompressed:compressed ratio, per entry and archive-wide.
    pub max_ratio: u64,
    /// Ratio checks apply only at or above this many uncompressed bytes, so a
    /// small, highly compressible JSON file never trips them. The archive-wide
    /// ratio still catches many small entries.
    pub ratio_min_bytes: u64,
    /// Maximum number of path components (`a/b/c.txt` has depth 3).
    pub max_depth: usize,
    /// Maximum entry name length in bytes, relative to the package root.
    pub max_path_len: usize,
    /// Maximum length of a single path component in bytes.
    pub max_component_len: usize,
    /// When `true` (the default), entry names must be ASCII. This removes
    /// every Unicode case-folding and normalization collision class.
    pub ascii_paths_only: bool,
    /// Maximum size of an HTML, SVG, CSS, JS or JSON file. These files are
    /// held in memory for the remote-reference scan; a larger one is refused
    /// rather than left unscanned.
    pub max_scanned_text_bytes: u64,
}

const MIB: u64 = 1024 * 1024;

impl Default for PackageLimits {
    fn default() -> Self {
        Self {
            max_archive_bytes: 256 * MIB,
            max_entries: 10_000,
            max_total_uncompressed: 512 * MIB,
            max_file_uncompressed: 128 * MIB,
            max_ratio: 100,
            ratio_min_bytes: MIB,
            max_depth: 16,
            max_path_len: 240,
            max_component_len: 128,
            ascii_paths_only: true,
            max_scanned_text_bytes: 4 * MIB,
        }
    }
}
