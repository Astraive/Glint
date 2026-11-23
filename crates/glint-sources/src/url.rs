//! Theme source string parser (SRC-002).
//!
//! Turns what a user types (CLI argument, manager text box, deep link
//! payload) into a [`ThemeSource`], or refuses it with a stable
//! `GLINT_SOURCE_URL_*` code. Parsing is pure: it never touches the network
//! or the filesystem.
//!
//! # Grammar
//!
//! Exactly one reading is possible for any accepted input:
//!
//! | Input | Variant |
//! |---|---|
//! | `registry/package` or `registry/package@1.2.3` (lowercase names, exact SemVer) | [`ThemeSource::Registry`] |
//! | `https://host/path.git`, optionally `#ref` or `#ref:subpath` | [`ThemeSource::Git`] |
//! | `git+https://host/path`, same fragment rules (no `.git` suffix needed) | [`ThemeSource::Git`] |
//! | `https://host/path.glint`, optionally `?query` | [`ThemeSource::ArtifactUrl`] |
//! | A path starting with `/`, `\`, `./`, `../`, `.\`, `..\`, `~/`, `~\`, a drive (`C:\`, `C:/`), or exactly `.`, `..` or `~` | [`ThemeSource::LocalArchive`] when it ends in `.glint`, else [`ThemeSource::LocalDirectory`] |
//!
//! The Git `#ref:subpath` fragment is **proposed syntax, not final public
//! syntax** (see `docs/plan/05-distribution/sources-and-download-strategies.md`);
//! [`ThemeSource::uses_proposed_syntax`] reports when a value depends on it.
//!
//! # Rejected on purpose
//!
//! - Every scheme other than `https` and `git+https`: `file:`, `http:`,
//!   `ssh:`, `git:`, `git+ssh:`, `ext::`, `fd::`, `javascript:`, `data:` ...
//!   and the SSH shorthand `user@host:path`.
//! - Leading `-` (option injection into a future fetcher), and Git refs or
//!   subpath components starting with `-`.
//! - Credentials in URLs (`user:token@host`); private Git credentials belong
//!   in the system credential manager (SRC-007). Error messages never echo
//!   the input, so a pasted token does not reach logs.
//! - IP-literal hosts (IPv4 in any notation, IPv6) and local names
//!   (`localhost`, `*.localhost`, `*.local`, `*.internal`, `*.home.arpa`,
//!   single-label hosts). This is a syntactic guard only: the fetcher must
//!   still refuse private and loopback addresses after DNS resolution
//!   (SRC-005), which also covers DNS rebinding.
//! - Non-ASCII hosts (homographs; use the `xn--` form), percent signs in
//!   hosts, dot segments, empty segments, backslashes and encoded `/`, `\`
//!   or control bytes in URL paths, raw non-ASCII in URLs.
//! - Network paths (`\\server\share`, `//server`, `\\?\` device paths), which
//!   would make Windows authenticate to a remote host.
//! - C0/C1 control characters anywhere, invisible and bidirectional
//!   formatting characters anywhere (they can make an input display as
//!   something it is not), surrounding whitespace, input longer than
//!   [`MAX_SOURCE_LEN`] bytes.
//! - Inputs that could mean two things: an `https` URL that ends in neither
//!   `.git` nor `.glint`, and bare words such as `midnight.glint` (write
//!   `./midnight.glint`).

use std::fmt;
use std::path::{Path, PathBuf};

use crate::ThemeSource;

/// Longest accepted source string, in bytes.
pub const MAX_SOURCE_LEN: usize = 2048;

const MAX_HOST_LEN: usize = 253;
const MAX_LABEL_LEN: usize = 63;
const MAX_NAME_LEN: usize = 64;
const MAX_REF_LEN: usize = 255;
const MAX_SUBPATH_LEN: usize = 512;
const MAX_SUBPATH_DEPTH: usize = 32;
const LOCAL_SUFFIXES: &[&str] = &["localhost", "local", "internal", "home.arpa"];

/// Stable, machine-readable source string error codes.
///
/// The string form (`GLINT_SOURCE_URL_*`) is the stable contract. Codes are
/// only ever added, never renamed or reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum SourceUrlErrorCode {
    /// The input is empty.
    Empty,
    /// The input is longer than [`MAX_SOURCE_LEN`] bytes.
    TooLong,
    /// A C0 or C1 control character (including NUL, tab, CR, LF, DEL).
    ControlChar,
    /// An invisible, zero-width or bidirectional formatting character.
    InvisibleChar,
    /// Leading or trailing whitespace.
    SurroundingWhitespace,
    /// The input starts with `-` and could be read as a command-line option.
    OptionLike,
    /// A scheme other than `https` or `git+https`.
    DisallowedScheme,
    /// The SSH shorthand `user@host:path`.
    SshShorthand,
    /// User name or password in a URL.
    Credentials,
    /// The URL host is empty, malformed, too long or not ASCII.
    InvalidHost,
    /// The URL host is an IP address literal.
    IpLiteral,
    /// The URL host is a local or single-label name.
    LocalHost,
    /// The URL port is empty, not a number or out of range.
    InvalidPort,
    /// The URL path has a disallowed character, escape or segment.
    InvalidPath,
    /// A Git URL carries a query string.
    QueryNotAllowed,
    /// An artifact URL carries a fragment.
    FragmentNotAllowed,
    /// The Git ref in `#ref[:subpath]` is invalid.
    InvalidReference,
    /// The Git subpath in `#ref:subpath` is invalid.
    InvalidSubpath,
    /// An `https` URL ends in neither `.git` nor `.glint`.
    AmbiguousUrl,
    /// Not a registry reference, URL or explicit local path.
    AmbiguousSource,
    /// A registry or package name is invalid.
    InvalidRegistryName,
    /// A registry version is not an exact SemVer version.
    InvalidVersion,
    /// A UNC, `//server` or device path.
    NetworkPath,
    /// A local path form that is not supported (`C:relative`, `~user/`).
    InvalidLocalPath,
    /// `~` was used but no home directory was provided.
    HomeUnavailable,
    /// The URL is structurally broken (e.g. `https:` without `//`).
    Malformed,
}

impl SourceUrlErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [SourceUrlErrorCode] = &[
        SourceUrlErrorCode::Empty,
        SourceUrlErrorCode::TooLong,
        SourceUrlErrorCode::ControlChar,
        SourceUrlErrorCode::InvisibleChar,
        SourceUrlErrorCode::SurroundingWhitespace,
        SourceUrlErrorCode::OptionLike,
        SourceUrlErrorCode::DisallowedScheme,
        SourceUrlErrorCode::SshShorthand,
        SourceUrlErrorCode::Credentials,
        SourceUrlErrorCode::InvalidHost,
        SourceUrlErrorCode::IpLiteral,
        SourceUrlErrorCode::LocalHost,
        SourceUrlErrorCode::InvalidPort,
        SourceUrlErrorCode::InvalidPath,
        SourceUrlErrorCode::QueryNotAllowed,
        SourceUrlErrorCode::FragmentNotAllowed,
        SourceUrlErrorCode::InvalidReference,
        SourceUrlErrorCode::InvalidSubpath,
        SourceUrlErrorCode::AmbiguousUrl,
        SourceUrlErrorCode::AmbiguousSource,
        SourceUrlErrorCode::InvalidRegistryName,
        SourceUrlErrorCode::InvalidVersion,
        SourceUrlErrorCode::NetworkPath,
        SourceUrlErrorCode::InvalidLocalPath,
        SourceUrlErrorCode::HomeUnavailable,
        SourceUrlErrorCode::Malformed,
    ];

    /// The stable string form, e.g. `GLINT_SOURCE_URL_CREDENTIALS`.
    pub const fn as_str(self) -> &'static str {
        match self {
            SourceUrlErrorCode::Empty => "GLINT_SOURCE_URL_EMPTY",
            SourceUrlErrorCode::TooLong => "GLINT_SOURCE_URL_TOO_LONG",
            SourceUrlErrorCode::ControlChar => "GLINT_SOURCE_URL_CONTROL_CHAR",
            SourceUrlErrorCode::InvisibleChar => "GLINT_SOURCE_URL_INVISIBLE_CHAR",
            SourceUrlErrorCode::SurroundingWhitespace => "GLINT_SOURCE_URL_SURROUNDING_WHITESPACE",
            SourceUrlErrorCode::OptionLike => "GLINT_SOURCE_URL_OPTION_LIKE",
            SourceUrlErrorCode::DisallowedScheme => "GLINT_SOURCE_URL_DISALLOWED_SCHEME",
            SourceUrlErrorCode::SshShorthand => "GLINT_SOURCE_URL_SSH_SHORTHAND",
            SourceUrlErrorCode::Credentials => "GLINT_SOURCE_URL_CREDENTIALS",
            SourceUrlErrorCode::InvalidHost => "GLINT_SOURCE_URL_INVALID_HOST",
            SourceUrlErrorCode::IpLiteral => "GLINT_SOURCE_URL_IP_LITERAL",
            SourceUrlErrorCode::LocalHost => "GLINT_SOURCE_URL_LOCAL_HOST",
            SourceUrlErrorCode::InvalidPort => "GLINT_SOURCE_URL_INVALID_PORT",
            SourceUrlErrorCode::InvalidPath => "GLINT_SOURCE_URL_INVALID_PATH",
            SourceUrlErrorCode::QueryNotAllowed => "GLINT_SOURCE_URL_QUERY_NOT_ALLOWED",
            SourceUrlErrorCode::FragmentNotAllowed => "GLINT_SOURCE_URL_FRAGMENT_NOT_ALLOWED",
            SourceUrlErrorCode::InvalidReference => "GLINT_SOURCE_URL_INVALID_REFERENCE",
            SourceUrlErrorCode::InvalidSubpath => "GLINT_SOURCE_URL_INVALID_SUBPATH",
            SourceUrlErrorCode::AmbiguousUrl => "GLINT_SOURCE_URL_AMBIGUOUS_URL",
            SourceUrlErrorCode::AmbiguousSource => "GLINT_SOURCE_URL_AMBIGUOUS_SOURCE",
            SourceUrlErrorCode::InvalidRegistryName => "GLINT_SOURCE_URL_INVALID_REGISTRY_NAME",
            SourceUrlErrorCode::InvalidVersion => "GLINT_SOURCE_URL_INVALID_VERSION",
            SourceUrlErrorCode::NetworkPath => "GLINT_SOURCE_URL_NETWORK_PATH",
            SourceUrlErrorCode::InvalidLocalPath => "GLINT_SOURCE_URL_INVALID_LOCAL_PATH",
            SourceUrlErrorCode::HomeUnavailable => "GLINT_SOURCE_URL_HOME_UNAVAILABLE",
            SourceUrlErrorCode::Malformed => "GLINT_SOURCE_URL_MALFORMED",
        }
    }
}

impl fmt::Display for SourceUrlErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A refused source string.
///
/// The message explains the rule that was broken; it never repeats the
/// input, which may contain a secret.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceUrlError {
    code: SourceUrlErrorCode,
    detail: &'static str,
}

impl SourceUrlError {
    fn new(code: SourceUrlErrorCode, detail: &'static str) -> Self {
        Self { code, detail }
    }

    /// The stable error code.
    pub fn code(&self) -> SourceUrlErrorCode {
        self.code
    }
}

impl fmt::Display for SourceUrlError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.code, self.detail)
    }
}

impl std::error::Error for SourceUrlError {}

/// Options for [`parse_theme_source_with`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct ParseOptions {
    /// Directory `~` expands to. Without it, `~` paths are refused with
    /// [`SourceUrlErrorCode::HomeUnavailable`].
    pub home_dir: Option<PathBuf>,
}

impl ParseOptions {
    /// Options with no home directory.
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the directory `~` expands to.
    #[must_use]
    pub fn with_home_dir(mut self, home: impl Into<PathBuf>) -> Self {
        self.home_dir = Some(home.into());
        self
    }
}

/// Parse a source string with default [`ParseOptions`] (no `~` expansion).
///
/// # Errors
///
/// A [`SourceUrlError`] with a stable `GLINT_SOURCE_URL_*` code.
pub fn parse_theme_source(input: &str) -> Result<ThemeSource, SourceUrlError> {
    parse_theme_source_with(input, &ParseOptions::default())
}

/// Parse a source string. See the [module documentation](self) for the
/// grammar and the rejected forms.
///
/// # Errors
///
/// A [`SourceUrlError`] with a stable `GLINT_SOURCE_URL_*` code.
pub fn parse_theme_source_with(
    input: &str,
    options: &ParseOptions,
) -> Result<ThemeSource, SourceUrlError> {
    use SourceUrlErrorCode as C;

    if input.is_empty() {
        return Err(SourceUrlError::new(C::Empty, "the source is empty"));
    }
    if input.len() > MAX_SOURCE_LEN {
        return Err(SourceUrlError::new(
            C::TooLong,
            "the source is longer than 2048 bytes",
        ));
    }
    if input.chars().any(char::is_control) {
        return Err(SourceUrlError::new(
            C::ControlChar,
            "control characters are not allowed",
        ));
    }
    if input.chars().any(is_invisible) {
        return Err(SourceUrlError::new(
            C::InvisibleChar,
            "invisible or bidirectional formatting characters are not allowed",
        ));
    }
    if input.starts_with(char::is_whitespace) || input.ends_with(char::is_whitespace) {
        return Err(SourceUrlError::new(
            C::SurroundingWhitespace,
            "leading or trailing whitespace is not allowed",
        ));
    }
    if input.starts_with('-') {
        return Err(SourceUrlError::new(
            C::OptionLike,
            "a source cannot start with '-'",
        ));
    }

    let bytes = input.as_bytes();
    let is_sep = |b: u8| b == b'/' || b == b'\\';
    if bytes.len() >= 2 && is_sep(bytes[0]) && is_sep(bytes[1]) {
        return Err(SourceUrlError::new(
            C::NetworkPath,
            "network and device paths are not allowed",
        ));
    }
    if is_sep(bytes[0])
        || matches!(input, "." | "..")
        || ["./", ".\\", "../", "..\\"]
            .iter()
            .any(|prefix| input.starts_with(prefix))
    {
        return Ok(local(PathBuf::from(input), input));
    }
    if let Some(rest) = input.strip_prefix('~') {
        return home_path(rest, input, options);
    }
    if bytes.len() >= 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' {
        if bytes.len() >= 3 && is_sep(bytes[2]) {
            return Ok(local(PathBuf::from(input), input));
        }
        return Err(SourceUrlError::new(
            C::InvalidLocalPath,
            "drive-relative paths are not supported; use C:\\ or C:/",
        ));
    }

    if let Some(colon) = input.find(':') {
        let before = &input[..colon];
        if !before.contains(['/', '\\']) {
            if is_scheme(before) {
                let scheme = before.to_ascii_lowercase();
                let rest = &input[colon + 1..];
                return match scheme.as_str() {
                    "https" => parse_https(rest, false),
                    "git+https" => parse_https(rest, true),
                    _ => Err(SourceUrlError::new(
                        C::DisallowedScheme,
                        "only https:// and git+https:// URLs are allowed",
                    )),
                };
            }
            if before.contains('@') {
                return Err(SourceUrlError::new(
                    C::SshShorthand,
                    "SSH sources (user@host:path) are not allowed; use https://",
                ));
            }
            return Err(SourceUrlError::new(
                C::Malformed,
                "unrecognized source form",
            ));
        }
    }

    parse_registry(input)
}

fn local(path: PathBuf, text: &str) -> ThemeSource {
    let lower = text.to_ascii_lowercase();
    if lower.ends_with(".glint") {
        ThemeSource::LocalArchive { path }
    } else {
        ThemeSource::LocalDirectory { path }
    }
}

fn home_path(
    rest: &str,
    input: &str,
    options: &ParseOptions,
) -> Result<ThemeSource, SourceUrlError> {
    use SourceUrlErrorCode as C;
    let relative = if rest.is_empty() {
        ""
    } else if let Some(stripped) = rest.strip_prefix(['/', '\\']) {
        stripped
    } else {
        return Err(SourceUrlError::new(
            C::InvalidLocalPath,
            "only ~ and ~/ are supported, not ~user",
        ));
    };
    let Some(home) = options.home_dir.as_deref() else {
        return Err(SourceUrlError::new(
            C::HomeUnavailable,
            "no home directory is available to expand ~",
        ));
    };
    let path = if relative.is_empty() {
        home.to_path_buf()
    } else {
        home.join(relative)
    };
    Ok(local(path, input))
}

/// RFC 3986 scheme: `ALPHA *( ALPHA / DIGIT / "+" / "-" / "." )`, at least
/// two characters so a drive letter is never a scheme.
fn is_scheme(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() >= 2
        && bytes[0].is_ascii_alphabetic()
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
}

fn is_invisible(c: char) -> bool {
    matches!(
        c,
        '\u{00AD}'
            | '\u{034F}'
            | '\u{061C}'
            | '\u{115F}'
            | '\u{1160}'
            | '\u{17B4}'
            | '\u{17B5}'
            | '\u{180B}'..='\u{180F}'
            | '\u{200B}'..='\u{200F}'
            | '\u{2028}'..='\u{202E}'
            | '\u{2060}'..='\u{206F}'
            | '\u{3164}'
            | '\u{FE00}'..='\u{FE0F}'
            | '\u{FEFF}'
            | '\u{FFA0}'
            | '\u{FFF0}'..='\u{FFFB}'
            | '\u{1D173}'..='\u{1D17A}'
            | '\u{E0000}'..='\u{E0FFF}'
    )
}

// ---------------------------------------------------------------------------
// Registry references
// ---------------------------------------------------------------------------

fn parse_registry(input: &str) -> Result<ThemeSource, SourceUrlError> {
    use SourceUrlErrorCode as C;
    let ambiguous = || {
        SourceUrlError::new(
            C::AmbiguousSource,
            "not a registry/package reference, URL or explicit path; prefix local paths with ./",
        )
    };
    if input.contains('\\') || input.to_ascii_lowercase().ends_with(".glint") {
        return Err(ambiguous());
    }
    let Some((registry, rest)) = input.split_once('/') else {
        return Err(ambiguous());
    };
    if rest.contains('/') {
        return Err(ambiguous());
    }
    let (package, version) = match rest.split_once('@') {
        Some((package, version)) => (package, Some(version)),
        None => (rest, None),
    };
    if !is_registry_name(registry) || !is_registry_name(package) {
        return Err(SourceUrlError::new(
            C::InvalidRegistryName,
            "registry and package names are 1-64 lowercase letters, digits and inner hyphens",
        ));
    }
    let version = match version {
        Some(text) => Some(
            semver::Version::parse(text)
                .map_err(|_| {
                    SourceUrlError::new(
                        C::InvalidVersion,
                        "the version must be an exact SemVer version such as 1.4.2",
                    )
                })?
                .to_string(),
        ),
        None => None,
    };
    Ok(ThemeSource::Registry {
        registry: registry.to_owned(),
        package: package.to_owned(),
        version,
    })
}

fn is_registry_name(name: &str) -> bool {
    let bytes = name.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_NAME_LEN
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes[0] != b'-'
        && bytes[bytes.len() - 1] != b'-'
}

// ---------------------------------------------------------------------------
// https:// and git+https:// URLs
// ---------------------------------------------------------------------------

fn parse_https(rest: &str, force_git: bool) -> Result<ThemeSource, SourceUrlError> {
    use SourceUrlErrorCode as C;
    let Some(after) = rest.strip_prefix("//") else {
        return Err(SourceUrlError::new(
            C::Malformed,
            "a URL must start with https://",
        ));
    };
    let authority_end = after.find(['/', '?', '#']).unwrap_or(after.len());
    let (authority, tail) = after.split_at(authority_end);
    let (before_fragment, fragment) = match tail.split_once('#') {
        Some((head, fragment)) => (head, Some(fragment)),
        None => (tail, None),
    };
    let (raw_path, query) = match before_fragment.split_once('?') {
        Some((path, query)) => (path, Some(query)),
        None => (before_fragment, None),
    };

    let (host, port) = parse_authority(authority)?;
    let path = normalize(if raw_path.is_empty() { "/" } else { raw_path }, false)
        .ok_or_else(|| invalid_path("the URL path has a disallowed character or escape"))?;
    let segments: Vec<&str> = if path == "/" {
        Vec::new()
    } else {
        path[1..].split('/').collect()
    };
    if segments.iter().any(|s| s.is_empty()) {
        return Err(invalid_path("empty path segments are not allowed"));
    }
    if segments.iter().any(|s| *s == "." || *s == "..") {
        return Err(invalid_path("dot segments are not allowed"));
    }
    let query =
        match query {
            Some(text) => Some(normalize(text, true).ok_or_else(|| {
                invalid_path("the URL query has a disallowed character or escape")
            })?),
            None => None,
        };

    let last = segments.last().map(|s| s.to_ascii_lowercase());
    let is_git_suffix = last
        .as_deref()
        .is_some_and(|s| s.len() > 4 && s.ends_with(".git"));
    let is_glint_suffix = last
        .as_deref()
        .is_some_and(|s| s.len() > 6 && s.ends_with(".glint"));

    let mut url = format!("https://{host}");
    if let Some(port) = port.filter(|port| *port != 443) {
        url.push_str(&format!(":{port}"));
    }
    url.push_str(&path);

    if force_git || is_git_suffix {
        if segments.is_empty() {
            return Err(invalid_path("a Git URL needs a repository path"));
        }
        if query.is_some() {
            return Err(SourceUrlError::new(
                C::QueryNotAllowed,
                "Git URLs cannot carry a query; use #ref:subpath",
            ));
        }
        let (reference, subpath) = match fragment {
            Some(fragment) => parse_git_fragment(fragment)?,
            None => (None, None),
        };
        return Ok(ThemeSource::Git {
            url,
            reference,
            subpath,
        });
    }
    if is_glint_suffix {
        if fragment.is_some() {
            return Err(SourceUrlError::new(
                C::FragmentNotAllowed,
                "artifact URLs cannot carry a fragment",
            ));
        }
        if let Some(query) = query {
            url.push('?');
            url.push_str(&query);
        }
        return Ok(ThemeSource::ArtifactUrl { url, sha256: None });
    }
    Err(SourceUrlError::new(
        C::AmbiguousUrl,
        "an https URL must end in .git (Git repository) or .glint (archive); use git+https:// for other Git URLs",
    ))
}

fn invalid_path(detail: &'static str) -> SourceUrlError {
    SourceUrlError::new(SourceUrlErrorCode::InvalidPath, detail)
}

fn parse_authority(authority: &str) -> Result<(String, Option<u16>), SourceUrlError> {
    use SourceUrlErrorCode as C;
    let invalid_host = |detail| SourceUrlError::new(C::InvalidHost, detail);
    if authority.contains('@') {
        return Err(SourceUrlError::new(
            C::Credentials,
            "credentials are not allowed in URLs; use the system credential manager",
        ));
    }
    if authority.starts_with('[') {
        return Err(SourceUrlError::new(
            C::IpLiteral,
            "IP address hosts are not allowed",
        ));
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(parse_port(port)?)),
        None => (authority, None),
    };
    if host.is_empty() {
        return Err(invalid_host("the URL has no host"));
    }
    if !host.is_ascii() {
        return Err(invalid_host(
            "hosts must be ASCII; use the xn-- form of internationalized names",
        ));
    }
    let host = host.to_ascii_lowercase();
    if host.len() > MAX_HOST_LEN {
        return Err(invalid_host("the host is longer than 253 characters"));
    }
    let labels: Vec<&str> = host.split('.').collect();
    let valid_label = |label: &&str| {
        let bytes = label.as_bytes();
        !bytes.is_empty()
            && bytes.len() <= MAX_LABEL_LEN
            && bytes
                .iter()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
            && bytes[0] != b'-'
            && bytes[bytes.len() - 1] != b'-'
    };
    if !labels.iter().all(valid_label) {
        return Err(invalid_host("the host is not a valid DNS name"));
    }
    let last = labels[labels.len() - 1];
    let numeric = last.bytes().all(|b| b.is_ascii_digit())
        || last
            .strip_prefix("0x")
            .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()));
    if numeric {
        return Err(SourceUrlError::new(
            C::IpLiteral,
            "IP address hosts are not allowed",
        ));
    }
    let local = labels.len() == 1
        || LOCAL_SUFFIXES.iter().any(|suffix| {
            host == *suffix
                || host
                    .strip_suffix(suffix)
                    .is_some_and(|head| head.ends_with('.'))
        });
    if local {
        return Err(SourceUrlError::new(
            C::LocalHost,
            "local and single-label hosts are not allowed",
        ));
    }
    Ok((host, port))
}

fn parse_port(text: &str) -> Result<u16, SourceUrlError> {
    let invalid = || {
        SourceUrlError::new(
            SourceUrlErrorCode::InvalidPort,
            "the port must be a number from 1 to 65535",
        )
    };
    if text.is_empty() || text.len() > 5 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(invalid());
    }
    match text.parse::<u16>() {
        Ok(port) if port != 0 => Ok(port),
        _ => Err(invalid()),
    }
}

/// Validate and normalize a URL path (`query == false`) or query: only
/// RFC 3986 `pchar` characters (plus `/`, and `?` in queries), unreserved
/// escapes decoded, other escapes uppercased. Encoded control bytes are
/// refused everywhere; encoded `/` and `\` are refused in paths.
fn normalize(text: &str, query: bool) -> Option<String> {
    let bytes = text.as_bytes();
    let mut out = String::with_capacity(text.len());
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if byte == b'%' {
            let hex = bytes.get(index + 1..index + 3)?;
            let value = u8::from_str_radix(std::str::from_utf8(hex).ok()?, 16).ok()?;
            if !hex.iter().all(u8::is_ascii_hexdigit) {
                return None;
            }
            if value < 0x20 || value == 0x7f {
                return None;
            }
            if !query && (value == b'/' || value == b'\\') {
                return None;
            }
            if is_unreserved(value) {
                out.push(char::from(value));
            } else {
                out.push('%');
                out.push_str(&format!("{value:02X}"));
            }
            index += 3;
            continue;
        }
        let allowed = is_unreserved(byte)
            || matches!(
                byte,
                b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+' | b',' | b';' | b'='
            )
            || matches!(byte, b':' | b'@' | b'/')
            || (query && byte == b'?');
        if !allowed {
            return None;
        }
        out.push(char::from(byte));
        index += 1;
    }
    Some(out)
}

fn is_unreserved(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'.' | b'_' | b'~')
}

// ---------------------------------------------------------------------------
// Proposed Git fragment: #ref[:subpath]
// ---------------------------------------------------------------------------

type GitFragment = (Option<String>, Option<String>);

fn parse_git_fragment(fragment: &str) -> Result<GitFragment, SourceUrlError> {
    let (reference, subpath) = match fragment.split_once(':') {
        Some((reference, subpath)) => (reference, Some(subpath)),
        None => (fragment, None),
    };
    if !is_git_ref(reference) {
        return Err(SourceUrlError::new(
            SourceUrlErrorCode::InvalidReference,
            "the Git ref must be a branch, tag or commit name (letters, digits, '.', '_', '-', '/')",
        ));
    }
    let subpath = match subpath {
        Some(text) => Some(normalize_subpath(text).ok_or_else(|| {
            SourceUrlError::new(
                SourceUrlErrorCode::InvalidSubpath,
                "the subpath must be a relative path of plain names inside the repository",
            )
        })?),
        None => None,
    };
    Ok((Some(reference.to_owned()), subpath))
}

/// A conservative subset of `git check-ref-format`.
fn is_git_ref(reference: &str) -> bool {
    let bytes = reference.as_bytes();
    !bytes.is_empty()
        && bytes.len() <= MAX_REF_LEN
        && bytes
            .iter()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-' | b'/'))
        && bytes[0] != b'-'
        && !reference.ends_with('/')
        && !reference.ends_with('.')
        && !reference.ends_with(".lock")
        && !reference.contains("..")
        && reference
            .split('/')
            .all(|part| !part.is_empty() && !part.starts_with('.'))
}

fn normalize_subpath(text: &str) -> Option<String> {
    let trimmed = text.strip_suffix('/').unwrap_or(text);
    if trimmed.is_empty() || trimmed.len() > MAX_SUBPATH_LEN {
        return None;
    }
    let parts: Vec<&str> = trimmed.split('/').collect();
    if parts.len() > MAX_SUBPATH_DEPTH {
        return None;
    }
    let valid = parts.iter().all(|part| {
        !part.is_empty()
            && *part != "."
            && *part != ".."
            && !part.starts_with('-')
            && !part.ends_with('.')
            && part
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
    });
    valid.then(|| trimmed.to_owned())
}

// ---------------------------------------------------------------------------
// Canonical form
// ---------------------------------------------------------------------------

impl ThemeSource {
    /// True when this value relies on the *proposed* (not final) Git
    /// `#ref:subpath` syntax.
    pub fn uses_proposed_syntax(&self) -> bool {
        matches!(
            self,
            ThemeSource::Git { reference, subpath, .. } if reference.is_some() || subpath.is_some()
        )
    }
}

/// The canonical source string. For registry, Git and artifact sources
/// produced by the parser, parsing the output gives the same value back.
impl fmt::Display for ThemeSource {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThemeSource::Registry {
                registry,
                package,
                version,
            } => {
                write!(f, "{registry}/{package}")?;
                if let Some(version) = version {
                    write!(f, "@{version}")?;
                }
                Ok(())
            }
            ThemeSource::Git {
                url,
                reference,
                subpath,
            } => {
                let explicit = url.to_ascii_lowercase().ends_with(".git");
                if !explicit {
                    f.write_str("git+")?;
                }
                f.write_str(url)?;
                if let Some(reference) = reference {
                    write!(f, "#{reference}")?;
                    if let Some(subpath) = subpath {
                        write!(f, ":{subpath}")?;
                    }
                }
                Ok(())
            }
            ThemeSource::LocalArchive { path } | ThemeSource::LocalDirectory { path } => {
                write!(f, "{}", Path::new(path).display())
            }
            ThemeSource::ArtifactUrl { url, .. } => f.write_str(url),
        }
    }
}
