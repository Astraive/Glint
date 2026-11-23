//! Source string table tests (SRC-002): every documented example parses to
//! the right `ThemeSource` variant, and dangerous schemes, credentials,
//! local hosts, IP literals, control and invisible characters, ambiguous
//! forms and overlong input are rejected with stable codes. No networking:
//! parsing never touches the network or the filesystem.
//!
//! Run with `cargo test -p glint-sources --test url_parse`.

use std::path::PathBuf;

use glint_sources::{
    MAX_SOURCE_LEN, ParseOptions, SourceUrlErrorCode as C, ThemeSource, parse_theme_source,
    parse_theme_source_with,
};

fn home() -> PathBuf {
    if cfg!(windows) {
        PathBuf::from(r"C:\Users\fixture")
    } else {
        PathBuf::from("/home/fixture")
    }
}

fn parse(input: &str) -> Result<ThemeSource, C> {
    parse_theme_source_with(input, &ParseOptions::new().with_home_dir(home()))
        .map_err(|error| error.code())
}

fn ok(input: &str) -> ThemeSource {
    parse(input).unwrap_or_else(|code| panic!("{input:?} should parse, got {code}"))
}

fn err(input: &str) -> C {
    match parse(input) {
        Ok(source) => panic!("{input:?} should be rejected, parsed as {source:?}"),
        Err(code) => code,
    }
}

fn registry(registry: &str, package: &str, version: Option<&str>) -> ThemeSource {
    ThemeSource::Registry {
        registry: registry.into(),
        package: package.into(),
        version: version.map(Into::into),
    }
}

fn git(url: &str, reference: Option<&str>, subpath: Option<&str>) -> ThemeSource {
    ThemeSource::Git {
        url: url.into(),
        reference: reference.map(Into::into),
        subpath: subpath.map(Into::into),
    }
}

fn artifact(url: &str) -> ThemeSource {
    ThemeSource::ArtifactUrl {
        url: url.into(),
        sha256: None,
    }
}

// ---------------------------------------------------------------------------
// Documented examples (docs/plan/05-distribution, "Theme source URL examples")
// ---------------------------------------------------------------------------

#[test]
fn documented_examples_parse_to_the_right_variant() {
    assert_eq!(ok("official/mono"), registry("official", "mono", None));
    assert_eq!(
        ok("community/carbon@1.4.2"),
        registry("community", "carbon", Some("1.4.2"))
    );
    assert_eq!(
        ok("https://github.com/user/glint-midnight.git"),
        git("https://github.com/user/glint-midnight.git", None, None)
    );
    assert_eq!(
        ok("https://github.com/user/themes.git#v1.0.0:themes/midnight"),
        git(
            "https://github.com/user/themes.git",
            Some("v1.0.0"),
            Some("themes/midnight")
        )
    );
    assert_eq!(
        ok("~/Downloads/midnight.glint"),
        ThemeSource::LocalArchive {
            path: home().join("Downloads/midnight.glint")
        }
    );
    assert_eq!(
        ok("./my-theme/"),
        ThemeSource::LocalDirectory {
            path: PathBuf::from("./my-theme/")
        }
    );
    assert_eq!(
        ok("https://example.org/midnight.glint"),
        artifact("https://example.org/midnight.glint")
    );
}

#[test]
fn ref_subpath_syntax_is_labelled_proposed() {
    assert!(ok("https://github.com/user/themes.git#v1.0.0:themes/midnight").uses_proposed_syntax());
    assert!(ok("https://github.com/user/themes.git#main").uses_proposed_syntax());
    assert!(!ok("https://github.com/user/themes.git").uses_proposed_syntax());
    assert!(!ok("official/mono").uses_proposed_syntax());
}

// ---------------------------------------------------------------------------
// Accepted forms and canonicalization
// ---------------------------------------------------------------------------

#[test]
fn registry_forms() {
    assert_eq!(
        ok("community/carbon@2.0.0-beta.1"),
        registry("community", "carbon", Some("2.0.0-beta.1"))
    );
    assert_eq!(ok("a1/b-2"), registry("a1", "b-2", None));
    for bad in [
        "Official/mono",
        "official/Mono",
        "official/mo_no",
        "official/-mono",
        "official/mono-",
        "official/mo.no",
        "official/",
    ] {
        let code = err(bad);
        assert!(
            matches!(
                code,
                C::InvalidRegistryName | C::AmbiguousSource | C::InvalidLocalPath
            ),
            "{bad:?} -> {code}"
        );
    }
    assert_eq!(err("official/Mono"), C::InvalidRegistryName);
    assert_eq!(err("official/mo_no"), C::InvalidRegistryName);
    assert_eq!(
        err(&format!("official/{}", "a".repeat(65))),
        C::InvalidRegistryName
    );
    for bad in [
        "official/mono@",
        "official/mono@1.4",
        "official/mono@v1.4.2",
        "official/mono@latest",
        "official/mono@>=1.0.0",
        "official/mono@1.0.0@2.0.0",
    ] {
        assert_eq!(err(bad), C::InvalidVersion, "{bad:?}");
    }
}

#[test]
fn git_forms_and_canonicalization() {
    // Scheme and host are case-insensitive; the default port is dropped.
    assert_eq!(
        ok("HTTPS://GitHub.COM:443/user/repo.git"),
        git("https://github.com/user/repo.git", None, None)
    );
    // Explicit git+https needs no .git suffix and is stored as https.
    assert_eq!(
        ok("git+https://codeberg.org/user/themes#main:themes/a"),
        git(
            "https://codeberg.org/user/themes",
            Some("main"),
            Some("themes/a")
        )
    );
    // Non-default port is kept.
    assert_eq!(
        ok("https://git.example.org:8443/team/repo.git"),
        git("https://git.example.org:8443/team/repo.git", None, None)
    );
    // Unreserved percent escapes are decoded, others uppercased.
    assert_eq!(
        ok("https://example.org/%7euser/a%2dtheme%c3%a9.git"),
        git("https://example.org/~user/a-theme%C3%A9.git", None, None)
    );
    // Ref only, refs with slashes, trailing subpath slash trimmed.
    assert_eq!(
        ok("https://example.org/r.git#release/1.x"),
        git("https://example.org/r.git", Some("release/1.x"), None)
    );
    assert_eq!(
        ok("https://example.org/r.git#0123abcd:themes/midnight/"),
        git(
            "https://example.org/r.git",
            Some("0123abcd"),
            Some("themes/midnight")
        )
    );
}

#[test]
fn artifact_forms() {
    assert_eq!(
        ok("https://example.org/dl/Midnight.GLINT"),
        artifact("https://example.org/dl/Midnight.GLINT")
    );
    assert_eq!(
        ok("https://example.org/dl/midnight.glint?token=abc&x=%2f"),
        artifact("https://example.org/dl/midnight.glint?token=abc&x=%2F")
    );
    assert_eq!(
        err("https://example.org/midnight.glint#sha256=00"),
        C::FragmentNotAllowed
    );
}

#[test]
fn local_path_forms() {
    let dir = |p: &str| ThemeSource::LocalDirectory {
        path: PathBuf::from(p),
    };
    let archive = |p: &str| ThemeSource::LocalArchive {
        path: PathBuf::from(p),
    };
    assert_eq!(ok("."), dir("."));
    assert_eq!(ok(".."), dir(".."));
    assert_eq!(ok("../themes/x"), dir("../themes/x"));
    assert_eq!(ok("/opt/themes/x.glint"), archive("/opt/themes/x.glint"));
    assert_eq!(ok(r".\themes\x.GLINT"), archive(r".\themes\x.GLINT"));
    assert_eq!(ok(r"C:\Themes\My Theme"), dir(r"C:\Themes\My Theme"));
    assert_eq!(ok("D:/themes/x.glint"), archive("D:/themes/x.glint"));
    assert_eq!(ok(r"\themes\x"), dir(r"\themes\x"));
    assert_eq!(ok("./Thèmes/ünï.glint"), archive("./Thèmes/ünï.glint"));
    assert_eq!(ok("~"), ThemeSource::LocalDirectory { path: home() });
    assert_eq!(
        ok(r"~\themes"),
        ThemeSource::LocalDirectory {
            path: home().join("themes")
        }
    );
}

#[test]
fn home_expansion_needs_a_home_dir() {
    let error = parse_theme_source("~/x.glint").unwrap_err();
    assert_eq!(error.code(), C::HomeUnavailable);
    assert_eq!(err("~user/x.glint"), C::InvalidLocalPath);
}

#[test]
fn canonical_display_round_trips() {
    for input in [
        "official/mono",
        "community/carbon@1.4.2",
        "https://github.com/user/glint-midnight.git",
        "https://github.com/user/themes.git#v1.0.0:themes/midnight",
        "git+https://codeberg.org/user/themes#main",
        "https://example.org/midnight.glint",
        "https://example.org:8443/a/midnight.glint?x=1",
    ] {
        let parsed = ok(input);
        let shown = parsed.to_string();
        assert_eq!(shown, input);
        assert_eq!(ok(&shown), parsed);
    }
    assert_eq!(
        ok("HTTPS://Example.org:443/a.glint").to_string(),
        "https://example.org/a.glint"
    );
}

// ---------------------------------------------------------------------------
// Rejections
// ---------------------------------------------------------------------------

#[test]
fn dangerous_schemes_are_rejected() {
    for bad in [
        "file:///etc/passwd",
        "file://C:/themes/x.glint",
        "FILE:x.glint",
        "ext::sh -c touch% /tmp/pwned",
        "ext::git-remote-x",
        "javascript:alert(1)",
        "JavaScript:alert(1)",
        "http://example.org/x.glint",
        "ssh://git@github.com/user/repo.git",
        "git+ssh://git@github.com/user/repo.git",
        "git://github.com/user/repo.git",
        "git+file:///repo",
        "data:application/zip;base64,UEsDBA==",
        "ftp://example.org/x.glint",
        "vbscript:msgbox",
        "fd::17",
        "glint://install/x",
    ] {
        assert_eq!(err(bad), C::DisallowedScheme, "{bad:?}");
    }
}

#[test]
fn ssh_tricks_are_rejected() {
    assert_eq!(err("git@github.com:user/repo.git"), C::SshShorthand);
    assert_eq!(err("user@host:repo"), C::SshShorthand);
    assert_eq!(err("-oProxyCommand=calc.exe"), C::OptionLike);
    assert_eq!(err("--upload-pack=touch /tmp/x"), C::OptionLike);
    assert_eq!(err("ssh://-oProxyCommand=calc/x.git"), C::DisallowedScheme);
    // An option-like ref is not a ref.
    assert_eq!(
        err("https://example.org/r.git#--upload-pack=x"),
        C::InvalidReference
    );
}

#[test]
fn credentials_are_rejected() {
    for bad in [
        "https://user:token@github.com/user/repo.git",
        "https://token@github.com/user/repo.git",
        "https://@github.com/user/repo.git",
        "https://user:pass@example.org/x.glint",
    ] {
        assert_eq!(err(bad), C::Credentials, "{bad:?}");
    }
}

#[test]
fn local_and_ip_hosts_are_rejected() {
    for bad in [
        "https://localhost/x.glint",
        "https://LOCALHOST/r.git",
        "https://api.localhost/x.glint",
        "https://printer.local/x.glint",
        "https://git.corp.internal/r.git",
        "https://nas.home.arpa/x.glint",
        "https://intranet/r.git",
    ] {
        assert_eq!(err(bad), C::LocalHost, "{bad:?}");
    }
    for bad in [
        "https://127.0.0.1/x.glint",
        "https://10.0.0.1/r.git",
        "https://192.168.1.10:8443/x.glint",
        "https://169.254.169.254/latest/x.glint",
        "https://0x7f.0.0.1/x.glint",
        "https://2130706433/x.glint",
        "https://017700000001/x.glint",
        "https://[::1]/x.glint",
        "https://[fe80::1%25eth0]/r.git",
        "https://[::ffff:127.0.0.1]/x.glint",
        "https://example.1/x.glint",
    ] {
        assert_eq!(err(bad), C::IpLiteral, "{bad:?}");
    }
}

#[test]
fn malformed_hosts_and_ports_are_rejected() {
    for bad in [
        "https:///x.glint",
        "https://exa_mple.org/x.glint",
        "https://-example.org/x.glint",
        "https://example-.org/x.glint",
        "https://example..org/x.glint",
        "https://example.org./x.glint",
        "https://.example.org/x.glint",
        "https://exa%6dple.org/x.glint",
        "https://ex\u{0430}mple.org/x.glint",
        "https://example\u{FF0E}org/x.glint",
        "https://ex ample.org/x.glint",
    ] {
        assert_eq!(err(bad), C::InvalidHost, "{bad:?}");
    }
    assert_eq!(
        err(&format!("https://{}.org/x.glint", "a".repeat(64))),
        C::InvalidHost
    );
    let long_host = vec!["abcdefghij"; 24].join(".") + ".org";
    assert_eq!(err(&format!("https://{long_host}/x.glint")), C::InvalidHost);
    for bad in [
        "https://example.org:/x.glint",
        "https://example.org:0/x.glint",
        "https://example.org:65536/x.glint",
        "https://example.org:44a3/x.glint",
        "https://example.org:+443/x.glint",
    ] {
        assert_eq!(err(bad), C::InvalidPort, "{bad:?}");
    }
}

#[test]
fn hostile_paths_are_rejected() {
    for bad in [
        "https://example.org/a/../b.glint",
        "https://example.org/a/./b.glint",
        "https://example.org/a/%2e%2e/b.glint",
        "https://example.org/a/%2E/b.glint",
        "https://example.org/a%2fb.glint",
        "https://example.org/a%5cb.glint",
        "https://example.org/a%00.glint",
        "https://example.org/a\\b.glint",
        "https://example.org/a//b.glint",
        "https://example.org/a b.glint",
        "https://example.org/a%zz.glint",
        "https://example.org/a%4.glint",
        "https://example.org/th\u{00E8}me.glint",
        "https://example.org/a<b>.glint",
        "https://example.org/a|b.glint",
    ] {
        assert_eq!(err(bad), C::InvalidPath, "{bad:?}");
    }
    assert_eq!(err("git+https://example.org/"), C::InvalidPath);
    assert_eq!(err("git+https://example.org"), C::InvalidPath);
}

#[test]
fn ambiguous_inputs_are_rejected() {
    // An https URL must say what it is.
    for bad in [
        "https://github.com/user/repo",
        "https://example.org/",
        "https://example.org",
        "https://example.org/x.zip",
    ] {
        assert_eq!(err(bad), C::AmbiguousUrl, "{bad:?}");
    }
    // A bare name is neither a registry reference nor an explicit path.
    for bad in ["midnight.glint", "midnight", "themes/a/b", "a/b.glint"] {
        assert_eq!(err(bad), C::AmbiguousSource, "{bad:?}");
    }
    assert_eq!(err("https:example.org/x.glint"), C::Malformed);
    assert_eq!(err("https:/example.org/x.glint"), C::Malformed);
}

#[test]
fn git_query_and_bad_fragments_are_rejected() {
    assert_eq!(
        err("https://example.org/r.git?ref=main"),
        C::QueryNotAllowed
    );
    for bad in [
        "https://example.org/r.git#",
        "https://example.org/r.git#:themes/a",
        "https://example.org/r.git#a..b",
        "https://example.org/r.git#/main",
        "https://example.org/r.git#main/",
        "https://example.org/r.git#.hidden",
        "https://example.org/r.git#a/.b",
        "https://example.org/r.git#main.lock",
        "https://example.org/r.git#ma in",
        "https://example.org/r.git#a@{1}",
        "https://example.org/r.git#a//b",
        "https://example.org/r.git#a%20b",
    ] {
        assert_eq!(err(bad), C::InvalidReference, "{bad:?}");
    }
    assert_eq!(
        err(&format!("https://example.org/r.git#{}", "a".repeat(256))),
        C::InvalidReference
    );
    for bad in [
        "https://example.org/r.git#main:",
        "https://example.org/r.git#main:/abs",
        "https://example.org/r.git#main:a/../b",
        "https://example.org/r.git#main:a/./b",
        "https://example.org/r.git#main:a//b",
        "https://example.org/r.git#main:a\\b",
        "https://example.org/r.git#main:a:b",
        "https://example.org/r.git#main:-a",
        "https://example.org/r.git#main:a./b",
        "https://example.org/r.git#main:a b",
    ] {
        assert_eq!(err(bad), C::InvalidSubpath, "{bad:?}");
    }
    let deep = vec!["d"; 33].join("/");
    assert_eq!(
        err(&format!("https://example.org/r.git#main:{deep}")),
        C::InvalidSubpath
    );
}

#[test]
fn network_paths_are_rejected() {
    for bad in [
        r"\\server\share\x.glint",
        r"\\?\C:\themes\x.glint",
        r"\\.\pipe\x",
        "//server/share/x.glint",
        r"/\server\share",
    ] {
        assert_eq!(err(bad), C::NetworkPath, "{bad:?}");
    }
    assert_eq!(err("C:relative"), C::InvalidLocalPath);
}

#[test]
fn control_and_invisible_characters_are_rejected() {
    for bad in [
        "official/mono\0",
        "official/\nmono",
        "https://example.org/x.glint\r\n",
        "https://example.org/\tx.glint",
        "./theme\u{7f}",
        "./theme\u{85}",
        "./theme\u{9b}x",
    ] {
        assert_eq!(err(bad), C::ControlChar, "{bad:?}");
    }
    for bad in [
        "./\u{202E}nilg.exe",
        "official/mo\u{200B}no",
        "https://exa\u{200D}mple.org/x.glint",
        "\u{FEFF}official/mono",
        "./a\u{2066}b",
        "./a\u{00AD}b",
        "./a\u{2028}b",
        "./a\u{E0041}b",
        "./a\u{FE0F}b",
    ] {
        assert_eq!(err(bad), C::InvisibleChar, "{bad:?}");
    }
}

#[test]
fn whitespace_empty_and_overlong_inputs_are_rejected() {
    assert_eq!(err(""), C::Empty);
    assert_eq!(err(" official/mono"), C::SurroundingWhitespace);
    assert_eq!(err("official/mono "), C::SurroundingWhitespace);
    assert_eq!(err("\u{3000}official/mono"), C::SurroundingWhitespace);
    let long = format!("https://example.org/{}.glint", "a".repeat(MAX_SOURCE_LEN));
    assert_eq!(err(&long), C::TooLong);
    let fits = format!(
        "https://example.org/{}.glint",
        "a".repeat(MAX_SOURCE_LEN - "https://example.org/.glint".len())
    );
    assert_eq!(fits.len(), MAX_SOURCE_LEN);
    assert!(matches!(ok(&fits), ThemeSource::ArtifactUrl { .. }));
}

#[test]
fn every_url_error_code_is_distinct_and_prefixed() {
    let mut seen = std::collections::BTreeSet::new();
    for code in C::ALL {
        assert!(code.as_str().starts_with("GLINT_SOURCE_URL_"), "{code:?}");
        assert!(seen.insert(code.as_str()), "duplicate {code:?}");
        assert_eq!(code.to_string(), code.as_str());
    }
}

#[test]
fn errors_never_echo_credentials() {
    let error = parse_theme_source("https://user:s3cret@github.com/u/r.git").unwrap_err();
    assert_eq!(error.code(), C::Credentials);
    assert!(!error.to_string().contains("s3cret"));
    assert!(!format!("{error:?}").contains("s3cret"));
}
