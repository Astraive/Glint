//! The semantic design token contract and `tokens.json` parser (THM-004).
//!
//! Run with `cargo test -p glint-schema --test tokens`.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use glint_schema::{
    BRAND_NAMES, CONTRAST_REQUIREMENTS, ColorScheme, Rgba, TOKEN_REGISTRY, TokenErrorCode,
    TokenKind, color_pointer, contrast_ratio, css_variable_name, is_brand_name, is_token_name,
    parse_tokens, token_spec,
};
use serde_json::{Value, json};

fn repo_file(relative: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(relative);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn default_doc() -> Value {
    serde_json::from_str(&repo_file("themes/default/theme/tokens.json")).unwrap()
}

fn text(doc: &Value) -> String {
    serde_json::to_string(doc).unwrap()
}

fn rejected(doc: &Value) -> (TokenErrorCode, String) {
    let err = parse_tokens(&text(doc)).expect_err("document must be rejected");
    (err.code(), err.pointer().to_owned())
}

/// Replace the value at a JSON Pointer.
fn with(mut doc: Value, pointer: &str, value: Value) -> Value {
    *doc.pointer_mut(pointer).expect("pointer exists") = value;
    doc
}

/// Remove an object member at a JSON Pointer.
fn without(mut doc: Value, parent: &str, key: &str) -> Value {
    doc.pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .expect("parent is an object")
        .remove(key)
        .expect("key exists");
    doc
}

/// Add an object member at a JSON Pointer.
fn adding(mut doc: Value, parent: &str, key: &str, value: Value) -> Value {
    doc.pointer_mut(parent)
        .and_then(Value::as_object_mut)
        .expect("parent is an object")
        .insert(key.to_owned(), value);
    doc
}

// ------------------------------------------------------------ default theme

#[test]
fn default_theme_tokens_validate() {
    let tokens = parse_tokens(&repo_file("themes/default/theme/tokens.json"))
        .expect("the default theme's tokens validate");
    assert_eq!(tokens.schema_version, 1);
    assert_eq!(tokens.default_scheme(), ColorScheme::Dark);
    assert_eq!(tokens.colors.len(), 2, "dark and light");
    let colour_tokens = TOKEN_REGISTRY
        .iter()
        .filter(|s| s.kind == TokenKind::Color)
        .count();
    for scheme in tokens.colors.values() {
        assert_eq!(scheme.len(), colour_tokens);
    }
    assert_eq!(tokens.values.len(), TOKEN_REGISTRY.len() - colour_tokens);
    assert_eq!(
        tokens.value(ColorScheme::Dark, "panel.height"),
        Some("36px")
    );
    assert_eq!(
        tokens.value(ColorScheme::Dark, "text.primary"),
        Some("#f5f6f7")
    );
    assert_eq!(
        tokens.value(ColorScheme::Light, "text.primary"),
        Some("#141518")
    );
}

/// Brand rule: the default theme's colours are neutral greys, never green.
#[test]
fn default_theme_colours_are_neutral() {
    let tokens = parse_tokens(&repo_file("themes/default/theme/tokens.json")).unwrap();
    for (scheme, colors) in &tokens.colors {
        for (name, value) in colors {
            let c = Rgba::parse_hex(value).unwrap();
            let spread = c.r.max(c.g).max(c.b) - c.r.min(c.g).min(c.b);
            assert!(
                spread <= 16,
                "{}/{name}: {value} is not a neutral grey",
                scheme.as_str()
            );
        }
    }
}

/// The static fallback panel hard-codes dark-scheme values with contract
/// variable names; they must equal the tokens.
#[test]
fn panel_css_mirrors_the_tokens() {
    let tokens = parse_tokens(&repo_file("themes/default/theme/tokens.json")).unwrap();
    let css = repo_file("themes/default/surfaces/panel/panel.css");
    let mut checked = 0;
    for spec in TOKEN_REGISTRY {
        let var = css_variable_name(spec.name);
        let declaration = format!("  {var}: ");
        if let Some(start) = css.find(&declaration) {
            let rest = &css[start + declaration.len()..];
            let value = &rest[..rest.find(';').unwrap()];
            assert_eq!(
                Some(value),
                tokens.value(ColorScheme::Dark, spec.name),
                "{var} in panel.css differs from tokens.json"
            );
            checked += 1;
        }
    }
    assert!(checked >= 8, "panel.css declares {checked} contract tokens");
    // Every contract variable the panel *uses* must be a real token.
    for (index, _) in css.match_indices("var(--glint-") {
        let name = &css[index + 4..];
        let name = &name[..name.find(')').unwrap()];
        assert!(
            TOKEN_REGISTRY
                .iter()
                .any(|s| css_variable_name(s.name) == name),
            "panel.css uses {name}, which is not in the contract"
        );
    }
}

// ----------------------------------------------------------------- registry

#[test]
fn registry_names_are_semantic_unique_and_collision_free() {
    let mut names = BTreeSet::new();
    let mut variables = BTreeSet::new();
    for spec in TOKEN_REGISTRY {
        assert!(names.insert(spec.name), "duplicate token {}", spec.name);
        assert!(
            variables.insert(css_variable_name(spec.name)),
            "CSS variable collision for {}",
            spec.name
        );
        assert!(!is_brand_name(spec.name), "{} is a brand name", spec.name);
        assert!(spec.name.contains('.'), "{} has no group", spec.name);
        assert!(
            spec.name
                .split('.')
                .all(|s| !s.is_empty() && s.bytes().all(|b| b.is_ascii_lowercase() || b == b'-')),
            "{} is not lowercase kebab segments",
            spec.name
        );
        assert!(!spec.description.is_empty());
    }
    for named in [
        "background.default",
        "text.primary",
        "surface.raised",
        "focus.ring",
        "panel.height",
    ] {
        assert!(is_token_name(named), "{named} must be in the contract");
    }
    assert_eq!(
        css_variable_name("text.on-accent"),
        "--glint-text-on-accent"
    );
    assert!(token_spec("obsidian").is_none());
}

#[test]
fn contrast_requirements_reference_colour_tokens() {
    for req in CONTRAST_REQUIREMENTS {
        for name in [req.foreground, req.background] {
            assert_eq!(token_spec(name).map(|s| s.kind), Some(TokenKind::Color));
        }
        assert!(req.minimum >= 3.0);
    }
}

#[test]
fn json_schema_lists_exactly_the_registry() {
    let schema: Value = serde_json::from_str(&repo_file("schemas/glint-tokens-v1.schema.json"))
        .expect("schema is JSON");
    let keys = |pointer: &str| -> BTreeSet<String> {
        schema
            .pointer(pointer)
            .and_then(Value::as_object)
            .unwrap_or_else(|| panic!("{pointer} missing"))
            .keys()
            .cloned()
            .collect()
    };
    let expect = |color: bool| -> BTreeSet<String> {
        TOKEN_REGISTRY
            .iter()
            .filter(|s| (s.kind == TokenKind::Color) == color)
            .map(|s| s.name.to_owned())
            .collect()
    };
    assert_eq!(keys("/$defs/colorScheme/properties"), expect(true));
    assert_eq!(keys("/properties/tokens/properties"), expect(false));
    let required = |pointer: &str| -> BTreeSet<String> {
        schema
            .pointer(pointer)
            .and_then(Value::as_array)
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(required("/$defs/colorScheme/required"), expect(true));
    assert_eq!(required("/properties/tokens/required"), expect(false));
    assert_eq!(schema["x-glint-status"], "proposed");
    assert_eq!(schema["additionalProperties"], false);
}

#[test]
fn error_codes_are_stable_and_unique() {
    let mut seen = BTreeSet::new();
    for code in TokenErrorCode::ALL {
        let name = code.as_str();
        assert!(name.starts_with("GLINT_TOKENS_"), "{name}");
        assert!(seen.insert(name), "duplicate {name}");
        assert_eq!(code.to_string(), name);
    }
    assert_eq!(
        TokenErrorCode::BrandName.as_str(),
        "GLINT_TOKENS_BRAND_NAME"
    );
}

// ------------------------------------------------------------ brand names

#[test]
fn brand_named_tokens_are_rejected() {
    for brand in BRAND_NAMES {
        assert!(is_brand_name(brand));
    }
    for name in [
        "obsidian",
        "Titanium",
        "surface.obsidian",
        "graphite-950",
        "glint.accent",
    ] {
        assert!(is_brand_name(name), "{name}");
        let colour = with(
            adding(default_doc(), "/colorSchemes/dark", name, json!("#000000")),
            "/schemaVersion",
            json!(1),
        );
        assert_eq!(
            rejected(&colour),
            (
                TokenErrorCode::BrandName,
                color_pointer(ColorScheme::Dark, name)
            )
        );
        let plain = adding(default_doc(), "/tokens", name, json!("4px"));
        assert_eq!(
            rejected(&plain),
            (TokenErrorCode::BrandName, format!("/tokens/{name}"))
        );
    }
    assert!(!is_brand_name("text.muted"));
    assert!(!is_brand_name("background.default"));
}

#[test]
fn the_old_provisional_palette_layout_is_rejected() {
    // THM-001 shipped `color.graphite-950` style names; the contract refuses them.
    let legacy = json!({
        "schemaVersion": 1,
        "colorSchemes": { "dark": { "graphite-950": "#141518" } },
        "tokens": {}
    });
    assert_eq!(
        rejected(&legacy),
        (
            TokenErrorCode::BrandName,
            "/colorSchemes/dark/graphite-950".to_owned()
        )
    );
    let legacy_root = json!({ "color": { "graphite-950": "#141518" }, "semantic": {} });
    assert_eq!(
        rejected(&legacy_root).0,
        TokenErrorCode::MissingSchemaVersion
    );
}

// --------------------------------------------------------------- structure

#[test]
fn malformed_documents_have_stable_codes() {
    assert_eq!(
        parse_tokens("{").unwrap_err().code(),
        TokenErrorCode::InvalidJson
    );
    assert_eq!(
        parse_tokens("[]").unwrap_err().code(),
        TokenErrorCode::NotAnObject
    );
    assert_eq!(
        parse_tokens(r#"{"schemaVersion":1,"schemaVersion":1}"#)
            .unwrap_err()
            .code(),
        TokenErrorCode::DuplicateKey
    );
    let huge = format!(
        r#"{{"schemaVersion":1,"$comment":"{}"}}"#,
        "x".repeat(300 * 1024)
    );
    assert_eq!(
        parse_tokens(&huge).unwrap_err().code(),
        TokenErrorCode::TooLarge
    );
    assert_eq!(
        rejected(&without(default_doc(), "", "schemaVersion")),
        (
            TokenErrorCode::MissingSchemaVersion,
            "/schemaVersion".into()
        )
    );
    assert_eq!(
        rejected(&with(default_doc(), "/schemaVersion", json!(2))).0,
        TokenErrorCode::UnsupportedSchemaVersion
    );
    assert_eq!(
        rejected(&with(default_doc(), "/schemaVersion", json!("1"))).0,
        TokenErrorCode::InvalidType
    );
    assert_eq!(
        rejected(&adding(default_doc(), "", "extra", json!(1))),
        (TokenErrorCode::UnknownField, "/extra".into())
    );
    assert_eq!(
        rejected(&without(default_doc(), "", "colorSchemes")).0,
        TokenErrorCode::InvalidColorScheme
    );
    assert_eq!(
        rejected(&with(default_doc(), "/colorSchemes", json!({}))).0,
        TokenErrorCode::InvalidColorScheme
    );
    assert_eq!(
        rejected(&adding(default_doc(), "/colorSchemes", "sepia", json!({}))),
        (
            TokenErrorCode::InvalidColorScheme,
            "/colorSchemes/sepia".into()
        )
    );
    assert_eq!(
        rejected(&without(default_doc(), "", "tokens")),
        (TokenErrorCode::MissingToken, "/tokens".into())
    );
    assert_eq!(
        rejected(&with(default_doc(), "/tokens", json!([]))).0,
        TokenErrorCode::InvalidType
    );
}

#[test]
fn one_scheme_is_enough_and_each_must_be_complete() {
    let dark_only = without(default_doc(), "/colorSchemes", "light");
    let tokens = parse_tokens(&text(&dark_only)).unwrap();
    assert_eq!(tokens.colors.len(), 1);
    let css = tokens.to_css();
    assert!(!css.contains("prefers-color-scheme"));

    let light_only = without(default_doc(), "/colorSchemes", "dark");
    let tokens = parse_tokens(&text(&light_only)).unwrap();
    assert_eq!(tokens.default_scheme(), ColorScheme::Light);
    assert!(tokens.to_css().contains("color-scheme: light;"));

    assert_eq!(
        rejected(&without(default_doc(), "/colorSchemes/light", "focus.ring")),
        (
            TokenErrorCode::MissingToken,
            "/colorSchemes/light/focus.ring".into()
        )
    );
    assert_eq!(
        rejected(&without(default_doc(), "/tokens", "panel.height")),
        (TokenErrorCode::MissingToken, "/tokens/panel.height".into())
    );
}

#[test]
fn unknown_and_misplaced_tokens_are_rejected() {
    assert_eq!(
        rejected(&adding(
            default_doc(),
            "/tokens",
            "panel.colour",
            json!("#000000")
        )),
        (TokenErrorCode::UnknownToken, "/tokens/panel.colour".into())
    );
    assert_eq!(
        rejected(&adding(
            default_doc(),
            "/colorSchemes/dark",
            "danger.default",
            json!("#ff0000")
        )),
        (
            TokenErrorCode::UnknownToken,
            "/colorSchemes/dark/danger.default".into()
        )
    );
    assert_eq!(
        rejected(&adding(
            default_doc(),
            "/colorSchemes/dark",
            "panel.height",
            json!("36px")
        )),
        (
            TokenErrorCode::InvalidValue,
            "/colorSchemes/dark/panel.height".into()
        )
    );
    assert_eq!(
        rejected(&adding(
            default_doc(),
            "/tokens",
            "text.primary",
            json!("#ffffff")
        )),
        (TokenErrorCode::InvalidValue, "/tokens/text.primary".into())
    );
}

// ------------------------------------------------------------------ values

#[test]
fn value_grammars_accept_and_reject() {
    let bad_values: &[(&str, Value)] = &[
        ("/colorSchemes/dark/text.primary", json!("white")),
        ("/colorSchemes/dark/text.primary", json!("#12345")),
        ("/colorSchemes/dark/text.primary", json!("url(https://x)")),
        ("/colorSchemes/dark/text.primary", json!(255)),
        ("/tokens/space.md", json!("12")),
        ("/tokens/space.md", json!("-4px")),
        ("/tokens/space.md", json!("calc(1px + 2px)")),
        ("/tokens/space.md", json!("12px; color: red")),
        ("/tokens/space.md", json!("var(--x)")),
        ("/tokens/font.family.base", json!("Segoe UI")),
        ("/tokens/font.family.base", json!("\"Segoe UI\"")),
        ("/tokens/font.family.base", json!("inherit, sans-serif")),
        ("/tokens/font.family.base", json!("\"A;B\", sans-serif")),
        ("/tokens/font.family.base", json!("url(x), sans-serif")),
        ("/tokens/font.weight.strong", json!("600")),
        ("/tokens/font.weight.strong", json!(0)),
        ("/tokens/font.weight.strong", json!(1500)),
        ("/tokens/font.weight.strong", json!(600.5)),
        ("/tokens/font.line-height.base", json!(9)),
        ("/tokens/font.line-height.base", json!("1.4")),
        ("/tokens/motion.duration.fast", json!("fast")),
        ("/tokens/motion.duration.fast", json!("10")),
        ("/tokens/motion.duration.fast", json!("6s")),
        ("/tokens/motion.easing.standard", json!("bounce")),
        (
            "/tokens/motion.easing.standard",
            json!("cubic-bezier(2, 0, 0, 1)"),
        ),
        (
            "/tokens/motion.easing.standard",
            json!("cubic-bezier(0, 0, 1)"),
        ),
        ("/tokens/elevation.low", json!("0 1px #000000")),
        ("/tokens/elevation.low", json!("0 1px 2px red")),
        ("/tokens/elevation.low", json!("0 1px -2px #000000")),
        ("/tokens/elevation.low", json!("0 1px 2px url(x)")),
        (
            "/tokens/elevation.low",
            json!("none, none, none, none, none"),
        ),
    ];
    for (pointer, value) in bad_values {
        let doc = with(default_doc(), pointer, value.clone());
        let (code, at) = rejected(&doc);
        assert!(
            matches!(
                code,
                TokenErrorCode::InvalidValue | TokenErrorCode::InvalidType
            ),
            "{pointer} = {value}: {code}"
        );
        assert_eq!(&at, pointer);
    }

    let good_values: &[(&str, Value)] = &[
        ("/colorSchemes/dark/accent.subtle", json!("#ABC")),
        ("/colorSchemes/dark/accent.subtle", json!("#aabbccdd")),
        ("/tokens/space.md", json!("0")),
        ("/tokens/space.md", json!("0.75rem")),
        ("/tokens/font.weight.strong", json!(700)),
        ("/tokens/font.line-height.base", json!(1.5)),
        ("/tokens/motion.duration.fast", json!("0.1s")),
        ("/tokens/motion.easing.standard", json!("ease-in-out")),
        ("/tokens/elevation.low", json!("none")),
        (
            "/tokens/elevation.low",
            json!("0 1px 2px #000, -1px 0 0 1px #00000040"),
        ),
        ("/tokens/font.family.base", json!("\"Segoe UI\", system-ui")),
    ];
    for (pointer, value) in good_values {
        let doc = with(default_doc(), pointer, value.clone());
        parse_tokens(&text(&doc)).unwrap_or_else(|e| panic!("{pointer} = {value}: {e}"));
    }
}

#[test]
fn numbers_are_normalised() {
    let doc = with(default_doc(), "/tokens/font.line-height.base", json!(1.50));
    let tokens = parse_tokens(&text(&doc)).unwrap();
    assert_eq!(
        tokens.value(ColorScheme::Dark, "font.line-height.base"),
        Some("1.5")
    );
    let upper = with(
        default_doc(),
        "/colorSchemes/dark/accent.subtle",
        json!("#ABCDEF"),
    );
    let tokens = parse_tokens(&text(&upper)).unwrap();
    assert_eq!(
        tokens.value(ColorScheme::Dark, "accent.subtle"),
        Some("#abcdef")
    );
}

// ---------------------------------------------------------------- contrast

#[test]
fn contrast_ratio_matches_wcag_reference_values() {
    let black = Rgba::parse_hex("#000").unwrap();
    let white = Rgba::parse_hex("#ffffff").unwrap();
    assert!((contrast_ratio(black, white) - 21.0).abs() < 1e-9);
    assert!((contrast_ratio(white, white) - 1.0).abs() < 1e-9);
    // Symmetric.
    assert!((contrast_ratio(white, black) - contrast_ratio(black, white)).abs() < 1e-12);
    // #767676 on white is the classic 4.54:1 AA boundary.
    let grey = Rgba::parse_hex("#767676").unwrap();
    let ratio = contrast_ratio(grey, white);
    assert!((4.5..4.6).contains(&ratio), "{ratio}");
    // #777777 on white is just below 4.5:1.
    let below = contrast_ratio(Rgba::parse_hex("#777777").unwrap(), white);
    assert!(below < 4.5, "{below}");
    assert_eq!(
        Rgba::parse_hex("#0f08"),
        Some(Rgba {
            r: 0,
            g: 255,
            b: 0,
            a: 136
        })
    );
    assert_eq!(Rgba::parse_hex("fff"), None);
    assert_eq!(Rgba::parse_hex("#ggg"), None);
    assert_eq!(Rgba::parse_hex("#12345"), None);
}

#[test]
fn low_contrast_is_rejected_per_scheme() {
    // Muted text almost the same as the background in the light scheme.
    let doc = with(
        default_doc(),
        "/colorSchemes/light/text.muted",
        json!("#dddddd"),
    );
    let (code, pointer) = rejected(&doc);
    assert_eq!(code, TokenErrorCode::LowContrast);
    assert_eq!(pointer, "/colorSchemes/light/text.muted");

    let doc = with(
        default_doc(),
        "/colorSchemes/dark/focus.ring",
        json!("#1b1c20"),
    );
    assert_eq!(
        rejected(&doc),
        (
            TokenErrorCode::LowContrast,
            "/colorSchemes/dark/focus.ring".into()
        )
    );

    let doc = with(
        default_doc(),
        "/colorSchemes/dark/text.on-accent",
        json!("#c4c7cd"),
    );
    assert_eq!(rejected(&doc).0, TokenErrorCode::LowContrast);
}

#[test]
fn translucent_colours_in_contrast_pairs_are_rejected() {
    let doc = with(
        default_doc(),
        "/colorSchemes/dark/text.primary",
        json!("#ffffff80"),
    );
    assert_eq!(
        rejected(&doc),
        (
            TokenErrorCode::InvalidValue,
            "/colorSchemes/dark/text.primary".into()
        )
    );
    // Translucent colours are fine where no contrast is required.
    let doc = with(
        default_doc(),
        "/colorSchemes/dark/border.subtle",
        json!("#ffffff1a"),
    );
    parse_tokens(&text(&doc)).unwrap();
}

#[test]
fn default_theme_meets_every_requirement_with_margin_report() {
    let tokens = parse_tokens(&repo_file("themes/default/theme/tokens.json")).unwrap();
    for (scheme, colors) in &tokens.colors {
        for req in CONTRAST_REQUIREMENTS {
            let fg = Rgba::parse_hex(&colors[req.foreground]).unwrap();
            let bg = Rgba::parse_hex(&colors[req.background]).unwrap();
            let ratio = contrast_ratio(fg, bg);
            assert!(
                ratio >= req.minimum,
                "{}: {} on {} is {ratio:.2}",
                scheme.as_str(),
                req.foreground,
                req.background
            );
        }
    }
}

// --------------------------------------------------------------------- css

#[test]
fn css_maps_tokens_to_custom_properties_with_scheme_and_motion_rules() {
    let tokens = parse_tokens(&repo_file("themes/default/theme/tokens.json")).unwrap();
    let css = tokens.to_css();
    assert_eq!(css, tokens.to_css(), "deterministic");

    // :root carries the dark default and every other token.
    let root = &css[..css.find("}\n").unwrap()];
    assert!(root.starts_with(":root {\n  color-scheme: dark;\n"));
    for spec in TOKEN_REGISTRY {
        let line = format!(
            "  {}: {};\n",
            css_variable_name(spec.name),
            tokens.value(ColorScheme::Dark, spec.name).unwrap()
        );
        assert!(root.contains(&line), "missing {line:?}");
    }
    assert!(root.contains("--glint-background-default: #141518;"));
    assert!(root.contains("--glint-panel-height: 36px;"));
    assert!(root.contains("--glint-font-weight-strong: 600;"));

    // Light scheme: media query for the system preference, explicit override.
    assert!(css.contains(
        "@media (prefers-color-scheme: light) {\n  :root:not([data-glint-scheme]) {\n    color-scheme: light;\n"
    ));
    assert!(css.contains(":root[data-glint-scheme=\"light\"] {\n  color-scheme: light;\n"));
    assert!(css.contains(":root[data-glint-scheme=\"dark\"] {\n  color-scheme: dark;\n"));
    assert!(css.contains("--glint-background-default: #ebecee;"));

    // Reduced motion zeroes every duration, via media query and attribute.
    for selector in [
        "@media (prefers-reduced-motion: reduce) {\n  :root {\n",
        ":root[data-glint-reduced-motion=\"true\"] {\n",
    ] {
        let start = css.find(selector).unwrap_or_else(|| panic!("{selector}"));
        let block = &css[start..];
        let block = &block[..block.find("}\n").unwrap()];
        for name in ["fast", "normal", "slow"] {
            assert!(
                block.contains(&format!("--glint-motion-duration-{name}: 0ms;")),
                "{selector} misses {name}"
            );
        }
        assert!(!block.contains("easing"), "easing is not zeroed");
    }

    // Only custom properties and baseline selectors: nothing fetchable.
    for forbidden in ["url(", "@import", "expression", "light-dark(", "color-mix("] {
        assert!(!css.contains(forbidden), "{forbidden}");
    }
    assert_eq!(css.matches('{').count(), css.matches('}').count());
}
