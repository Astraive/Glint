//! The **proposed** semantic design token contract and `tokens.json` v1
//! (THM-004).
//!
//! A theme's `tokens` file supplies values for a fixed registry of
//! *semantic* names such as `background.default`, `text.primary`,
//! `surface.raised`, `focus.ring` and `panel.height` ([`TOKEN_REGISTRY`]).
//! Names describe a role, never a palette entry: Glint brand constants such
//! as `Obsidian` or `Titanium` are refused ([`TokenErrorCode::BrandName`]).
//! The registry is closed: an unknown name is refused rather than ignored,
//! because surfaces and the SDK can only consume names the contract defines.
//!
//! # File shape
//!
//! ```json
//! {
//!   "schemaVersion": 1,
//!   "colorSchemes": { "dark": { "background.default": "#141518" } },
//!   "tokens": { "space.md": "12px" }
//! }
//! ```
//!
//! (The example is abbreviated: every token is required.) Colour tokens live
//! per scheme (`dark`, `light`; at least one) and every declared scheme must
//! define every colour token. All other tokens are scheme independent and all
//! are required. Colours are hexadecimal only (`#rgb`, `#rgba`, `#rrggbb`,
//! `#rrggbbaa`); other values follow the small grammars of [`TokenKind`]. No
//! value can carry a URL, a function other than `cubic-bezier`, or markup, so
//! a token value can never fetch anything.
//!
//! # Contrast
//!
//! [`CONTRAST_REQUIREMENTS`] lists WCAG 2.x pairs (4.5:1 for text, 3:1 for
//! the focus ring). [`parse_tokens`] refuses a scheme that misses one
//! ([`TokenErrorCode::LowContrast`]); [`contrast_ratio`] is public for
//! editors.
//!
//! # CSS mapping
//!
//! [`TokenFile::to_css`] renders the CSS custom properties described in
//! `docs/architecture/design-tokens.md`: `background.default` becomes
//! `--glint-background-default`. Only baseline (tier 0) CSS features are
//! used. Reduced motion zeroes every `motion.duration.*` variable.

use std::collections::BTreeMap;
use std::fmt;

use serde_json::{Number, Value};

use crate::error::{ErrorCode, ManifestError};
use crate::parse::{Obj, as_object, child_pointer, load_json_object, wrong_type};

/// The only `schemaVersion` of `tokens.json` this crate understands.
pub const SUPPORTED_TOKENS_SCHEMA_VERSION: u32 = 1;

/// Largest accepted tokens file, in bytes (256 KiB).
pub const MAX_TOKENS_BYTES: usize = 256 * 1024;

/// Prefix of every generated CSS custom property.
pub const CSS_PREFIX: &str = "--glint-";

/// Name words that are Glint brand constants and never public token names
/// (`docs/plan/01-product/brand-and-design-language.md`). Matched
/// case-insensitively against each `.`- or `-`-separated word of a name.
pub const BRAND_NAMES: &[&str] = &[
    "obsidian", "graphite", "gunmetal", "titanium", "silver", "chrome", "frost", "glint",
];

const TOP_LEVEL_FIELDS: &[&str] = &[
    "$schema",
    "$comment",
    "schemaVersion",
    "colorSchemes",
    "tokens",
];
const MAX_COMMENT_LEN: usize = 2048;
const MAX_VALUE_LEN: usize = 256;

// ---------------------------------------------------------------- registry

/// The value grammar of a token.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[non_exhaustive]
pub enum TokenKind {
    /// `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`. Per colour scheme.
    Color,
    /// A non-negative length: `0` or a number with `px`, `rem` or `em`.
    Dimension,
    /// A comma-separated font stack of quoted or bare family names ending in
    /// a CSS generic family.
    FontFamily,
    /// A JSON integer from 1 to 1000.
    FontWeight,
    /// A JSON number from 0.5 to 3 (unitless line height).
    Number,
    /// `<number>ms` or `<number>s`, at most 5 seconds.
    Duration,
    /// `linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out` or
    /// `cubic-bezier(x1, y1, x2, y2)` with `x1` and `x2` in 0..=1.
    Easing,
    /// `none` or one to four comma-separated layers of
    /// `<x> <y> <blur> [<spread>] <hex colour>`.
    Shadow,
}

/// One token of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TokenSpec {
    /// The public semantic name, dot-separated.
    pub name: &'static str,
    /// Its value grammar.
    pub kind: TokenKind,
    /// What the token is for.
    pub description: &'static str,
}

const fn spec(name: &'static str, kind: TokenKind, description: &'static str) -> TokenSpec {
    TokenSpec {
        name,
        kind,
        description,
    }
}

/// Every token of the contract, in the order CSS is generated. Groups:
/// `background`, `surface`, `text`, `border`, `accent`, `focus` (colour);
/// `font` (typography); `space`; `radius`; `elevation`; `motion`; `panel`.
pub const TOKEN_REGISTRY: &[TokenSpec] = &[
    spec(
        "background.default",
        TokenKind::Color,
        "Window or desktop backdrop behind surfaces.",
    ),
    spec(
        "background.subtle",
        TokenKind::Color,
        "Quiet variation of the backdrop (wells, zebra rows).",
    ),
    spec(
        "surface.default",
        TokenKind::Color,
        "Base surface such as a panel or dock.",
    ),
    spec(
        "surface.raised",
        TokenKind::Color,
        "Surface above the base: cards, menus, hovered controls.",
    ),
    spec(
        "surface.overlay",
        TokenKind::Color,
        "Surface above everything else: dialogs, popovers.",
    ),
    spec("text.primary", TokenKind::Color, "Main text and icons."),
    spec("text.secondary", TokenKind::Color, "Supporting text."),
    spec(
        "text.muted",
        TokenKind::Color,
        "De-emphasised metadata; still readable (4.5:1).",
    ),
    spec(
        "text.on-accent",
        TokenKind::Color,
        "Text and icons drawn on accent.default.",
    ),
    spec("border.subtle", TokenKind::Color, "Hairline separators."),
    spec(
        "border.default",
        TokenKind::Color,
        "Control and surface outlines.",
    ),
    spec(
        "accent.default",
        TokenKind::Color,
        "Emphasis colour for selected or primary controls.",
    ),
    spec(
        "accent.subtle",
        TokenKind::Color,
        "Low-emphasis accent fill (selection backgrounds).",
    ),
    spec(
        "focus.ring",
        TokenKind::Color,
        "Keyboard focus indicator (3:1 against surfaces).",
    ),
    spec(
        "font.family.base",
        TokenKind::FontFamily,
        "UI font stack (system fonts only).",
    ),
    spec(
        "font.family.mono",
        TokenKind::FontFamily,
        "Monospace font stack.",
    ),
    spec("font.size.small", TokenKind::Dimension, "Small text size."),
    spec("font.size.base", TokenKind::Dimension, "Body text size."),
    spec("font.size.large", TokenKind::Dimension, "Large text size."),
    spec(
        "font.weight.regular",
        TokenKind::FontWeight,
        "Regular text weight.",
    ),
    spec(
        "font.weight.strong",
        TokenKind::FontWeight,
        "Emphasised text weight.",
    ),
    spec(
        "font.line-height.base",
        TokenKind::Number,
        "Unitless body line height.",
    ),
    spec("space.xs", TokenKind::Dimension, "Smallest gap."),
    spec("space.sm", TokenKind::Dimension, "Small gap."),
    spec("space.md", TokenKind::Dimension, "Medium gap."),
    spec("space.lg", TokenKind::Dimension, "Large gap."),
    spec("space.xl", TokenKind::Dimension, "Largest gap."),
    spec("radius.sm", TokenKind::Dimension, "Small corner radius."),
    spec("radius.md", TokenKind::Dimension, "Medium corner radius."),
    spec("radius.lg", TokenKind::Dimension, "Large corner radius."),
    spec(
        "border.width",
        TokenKind::Dimension,
        "Width of default borders.",
    ),
    spec(
        "focus.width",
        TokenKind::Dimension,
        "Width of the focus ring.",
    ),
    spec(
        "focus.offset",
        TokenKind::Dimension,
        "Gap between a control and its focus ring.",
    ),
    spec(
        "elevation.low",
        TokenKind::Shadow,
        "Shadow of raised surfaces.",
    ),
    spec(
        "elevation.medium",
        TokenKind::Shadow,
        "Shadow of menus and popovers.",
    ),
    spec("elevation.high", TokenKind::Shadow, "Shadow of dialogs."),
    spec(
        "motion.duration.fast",
        TokenKind::Duration,
        "Micro-interactions. Zero under reduced motion.",
    ),
    spec(
        "motion.duration.normal",
        TokenKind::Duration,
        "Standard transitions. Zero under reduced motion.",
    ),
    spec(
        "motion.duration.slow",
        TokenKind::Duration,
        "Large transitions. Zero under reduced motion.",
    ),
    spec(
        "motion.easing.standard",
        TokenKind::Easing,
        "Default easing.",
    ),
    spec(
        "motion.easing.emphasized",
        TokenKind::Easing,
        "Easing for entrances and attention.",
    ),
    spec(
        "panel.height",
        TokenKind::Dimension,
        "Height of the top panel.",
    ),
];

/// Look a token up by its semantic name.
pub fn token_spec(name: &str) -> Option<&'static TokenSpec> {
    TOKEN_REGISTRY.iter().find(|spec| spec.name == name)
}

/// Whether `name` is a token of the contract.
pub fn is_token_name(name: &str) -> bool {
    token_spec(name).is_some()
}

/// The CSS custom property for a token name: `--glint-` plus the name with
/// `.` replaced by `-` (`text.on-accent` -> `--glint-text-on-accent`).
pub fn css_variable_name(name: &str) -> String {
    format!("{CSS_PREFIX}{}", name.replace('.', "-"))
}

/// Whether any word of `name` is a Glint brand constant.
pub fn is_brand_name(name: &str) -> bool {
    name.split(['.', '-', '_', ' '])
        .any(|word| BRAND_NAMES.iter().any(|b| word.eq_ignore_ascii_case(b)))
}

/// A colour scheme a theme may define colours for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum ColorScheme {
    /// Dark. The default when a theme defines both.
    Dark,
    /// Light.
    Light,
}

impl ColorScheme {
    /// The key used in `colorSchemes` and in CSS `color-scheme`.
    pub const fn as_str(self) -> &'static str {
        match self {
            ColorScheme::Dark => "dark",
            ColorScheme::Light => "light",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        match name {
            "dark" => Some(ColorScheme::Dark),
            "light" => Some(ColorScheme::Light),
            _ => None,
        }
    }
}

// ---------------------------------------------------------------- contrast

/// A colour with 8-bit channels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgba {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha (255 is opaque).
    pub a: u8,
}

impl Rgba {
    /// Parse `#rgb`, `#rgba`, `#rrggbb` or `#rrggbbaa`.
    pub fn parse_hex(text: &str) -> Option<Rgba> {
        let digits = text.strip_prefix('#')?;
        if !digits.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let pair = |i: usize| u8::from_str_radix(&digits[i..i + 2], 16).ok();
        let nibble = |i: usize| u8::from_str_radix(&digits[i..=i], 16).ok().map(|n| n * 17);
        match digits.len() {
            3 => Some(Rgba {
                r: nibble(0)?,
                g: nibble(1)?,
                b: nibble(2)?,
                a: 255,
            }),
            4 => Some(Rgba {
                r: nibble(0)?,
                g: nibble(1)?,
                b: nibble(2)?,
                a: nibble(3)?,
            }),
            6 => Some(Rgba {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: 255,
            }),
            8 => Some(Rgba {
                r: pair(0)?,
                g: pair(2)?,
                b: pair(4)?,
                a: pair(6)?,
            }),
            _ => None,
        }
    }

    /// WCAG 2.x relative luminance of the colour, ignoring alpha.
    pub fn relative_luminance(self) -> f64 {
        fn linear(channel: u8) -> f64 {
            let c = f64::from(channel) / 255.0;
            if c <= 0.04045 {
                c / 12.92
            } else {
                ((c + 0.055) / 1.055).powf(2.4)
            }
        }
        0.2126 * linear(self.r) + 0.7152 * linear(self.g) + 0.0722 * linear(self.b)
    }
}

/// WCAG 2.x contrast ratio between two colours, from 1 to 21. Alpha is
/// ignored: callers must pass opaque colours (see [`parse_tokens`]).
pub fn contrast_ratio(a: Rgba, b: Rgba) -> f64 {
    let (la, lb) = (a.relative_luminance(), b.relative_luminance());
    let (light, dark) = if la >= lb { (la, lb) } else { (lb, la) };
    (light + 0.05) / (dark + 0.05)
}

/// A minimum contrast every colour scheme must meet.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContrastRequirement {
    /// Foreground token.
    pub foreground: &'static str,
    /// Background token.
    pub background: &'static str,
    /// Minimum ratio (4.5 for text, 3 for non-text indicators).
    pub minimum: f64,
}

const fn contrast(
    foreground: &'static str,
    background: &'static str,
    minimum: f64,
) -> ContrastRequirement {
    ContrastRequirement {
        foreground,
        background,
        minimum,
    }
}

/// Contrast every colour scheme must meet (WCAG 2.x AA).
pub const CONTRAST_REQUIREMENTS: &[ContrastRequirement] = &[
    contrast("text.primary", "background.default", 4.5),
    contrast("text.primary", "surface.default", 4.5),
    contrast("text.primary", "surface.raised", 4.5),
    contrast("text.primary", "surface.overlay", 4.5),
    contrast("text.secondary", "background.default", 4.5),
    contrast("text.secondary", "surface.default", 4.5),
    contrast("text.secondary", "surface.raised", 4.5),
    contrast("text.muted", "background.default", 4.5),
    contrast("text.muted", "surface.default", 4.5),
    contrast("text.on-accent", "accent.default", 4.5),
    contrast("focus.ring", "background.default", 3.0),
    contrast("focus.ring", "surface.default", 3.0),
    contrast("focus.ring", "surface.raised", 3.0),
];

// ------------------------------------------------------------------ errors

/// Stable, machine-readable `tokens.json` error codes. The string form is the
/// contract; codes are only ever added.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[non_exhaustive]
pub enum TokenErrorCode {
    /// Not well-formed JSON.
    InvalidJson,
    /// Larger than [`MAX_TOKENS_BYTES`].
    TooLarge,
    /// An object repeats a key.
    DuplicateKey,
    /// The root is not a JSON object.
    NotAnObject,
    /// `schemaVersion` is absent.
    MissingSchemaVersion,
    /// `schemaVersion` is not [`SUPPORTED_TOKENS_SCHEMA_VERSION`].
    UnsupportedSchemaVersion,
    /// A top-level field is not part of the v1 format.
    UnknownField,
    /// A field has the wrong JSON type.
    InvalidType,
    /// A value does not match its token's grammar, or a token is in the
    /// wrong section.
    InvalidValue,
    /// A token name is not in the contract.
    UnknownToken,
    /// A token name uses a Glint brand constant such as `obsidian`.
    BrandName,
    /// A required token is absent; the pointer names where it belongs.
    MissingToken,
    /// `colorSchemes` is absent, empty or names a scheme other than `dark`
    /// or `light`.
    InvalidColorScheme,
    /// A colour scheme misses a [`CONTRAST_REQUIREMENTS`] pair.
    LowContrast,
}

impl TokenErrorCode {
    /// Every code, in declaration order.
    pub const ALL: &'static [TokenErrorCode] = &[
        TokenErrorCode::InvalidJson,
        TokenErrorCode::TooLarge,
        TokenErrorCode::DuplicateKey,
        TokenErrorCode::NotAnObject,
        TokenErrorCode::MissingSchemaVersion,
        TokenErrorCode::UnsupportedSchemaVersion,
        TokenErrorCode::UnknownField,
        TokenErrorCode::InvalidType,
        TokenErrorCode::InvalidValue,
        TokenErrorCode::UnknownToken,
        TokenErrorCode::BrandName,
        TokenErrorCode::MissingToken,
        TokenErrorCode::InvalidColorScheme,
        TokenErrorCode::LowContrast,
    ];

    /// The stable string form, e.g. `GLINT_TOKENS_BRAND_NAME`.
    pub const fn as_str(self) -> &'static str {
        match self {
            TokenErrorCode::InvalidJson => "GLINT_TOKENS_INVALID_JSON",
            TokenErrorCode::TooLarge => "GLINT_TOKENS_TOO_LARGE",
            TokenErrorCode::DuplicateKey => "GLINT_TOKENS_DUPLICATE_KEY",
            TokenErrorCode::NotAnObject => "GLINT_TOKENS_NOT_AN_OBJECT",
            TokenErrorCode::MissingSchemaVersion => "GLINT_TOKENS_MISSING_SCHEMA_VERSION",
            TokenErrorCode::UnsupportedSchemaVersion => "GLINT_TOKENS_UNSUPPORTED_SCHEMA_VERSION",
            TokenErrorCode::UnknownField => "GLINT_TOKENS_UNKNOWN_FIELD",
            TokenErrorCode::InvalidType => "GLINT_TOKENS_INVALID_TYPE",
            TokenErrorCode::InvalidValue => "GLINT_TOKENS_INVALID_VALUE",
            TokenErrorCode::UnknownToken => "GLINT_TOKENS_UNKNOWN_TOKEN",
            TokenErrorCode::BrandName => "GLINT_TOKENS_BRAND_NAME",
            TokenErrorCode::MissingToken => "GLINT_TOKENS_MISSING_TOKEN",
            TokenErrorCode::InvalidColorScheme => "GLINT_TOKENS_INVALID_COLOR_SCHEME",
            TokenErrorCode::LowContrast => "GLINT_TOKENS_LOW_CONTRAST",
        }
    }
}

impl fmt::Display for TokenErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A tokens rejection: a stable [`TokenErrorCode`], the JSON Pointer of the
/// offending value and a message (not stable).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TokenError {
    code: TokenErrorCode,
    pointer: String,
    message: String,
}

impl TokenError {
    fn new(code: TokenErrorCode, pointer: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code,
            pointer: pointer.into(),
            message: message.into(),
        }
    }

    /// The stable error code.
    pub fn code(&self) -> TokenErrorCode {
        self.code
    }

    /// JSON Pointer (RFC 6901) to the offending value; `""` is the root.
    pub fn pointer(&self) -> &str {
        &self.pointer
    }

    /// Human-readable detail (not stable).
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for TokenError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let at = if self.pointer.is_empty() {
            "(root)"
        } else {
            self.pointer.as_str()
        };
        write!(f, "{} at {}: {}", self.code, at, self.message)
    }
}

impl std::error::Error for TokenError {}

impl From<ManifestError> for TokenError {
    fn from(err: ManifestError) -> Self {
        let code = match err.code() {
            ErrorCode::InvalidJson => TokenErrorCode::InvalidJson,
            ErrorCode::TooLarge => TokenErrorCode::TooLarge,
            ErrorCode::DuplicateKey => TokenErrorCode::DuplicateKey,
            ErrorCode::NotAnObject => TokenErrorCode::NotAnObject,
            ErrorCode::UnknownField => TokenErrorCode::UnknownField,
            ErrorCode::InvalidType => TokenErrorCode::InvalidType,
            _ => TokenErrorCode::InvalidValue,
        };
        TokenError::new(code, err.pointer(), err.message())
    }
}

type Result<T> = std::result::Result<T, TokenError>;

// ------------------------------------------------------------------- types

/// A `tokens.json` that passed [`parse_tokens`].
#[derive(Debug, Clone, PartialEq)]
#[non_exhaustive]
pub struct TokenFile {
    /// Always [`SUPPORTED_TOKENS_SCHEMA_VERSION`].
    pub schema_version: u32,
    /// Colour tokens per scheme; each scheme has every colour token.
    pub colors: BTreeMap<ColorScheme, BTreeMap<String, String>>,
    /// Every non-colour token, normalised to its CSS text.
    pub values: BTreeMap<String, String>,
}

impl TokenFile {
    /// The scheme used when the user expresses no preference: dark when
    /// defined, otherwise light.
    pub fn default_scheme(&self) -> ColorScheme {
        if self.colors.contains_key(&ColorScheme::Dark) {
            ColorScheme::Dark
        } else {
            ColorScheme::Light
        }
    }

    /// The CSS text of a token in a scheme (scheme ignored for non-colours).
    pub fn value(&self, scheme: ColorScheme, name: &str) -> Option<&str> {
        self.colors
            .get(&scheme)
            .and_then(|colors| colors.get(name))
            .or_else(|| self.values.get(name))
            .map(String::as_str)
    }

    /// Render the CSS custom properties (see the module docs and
    /// `docs/architecture/design-tokens.md`).
    ///
    /// Output is deterministic and uses only custom properties, media
    /// queries and attribute selectors:
    ///
    /// 1. `:root` holds the default scheme's colours and all other tokens.
    /// 2. With two schemes, the other one applies under
    ///    `@media (prefers-color-scheme: ...)` when `<html>` has no
    ///    `data-glint-scheme`, and always for `[data-glint-scheme="..."]`.
    /// 3. Under `prefers-reduced-motion: reduce`, or
    ///    `[data-glint-reduced-motion="true"]`, every `motion.duration.*`
    ///    variable is `0ms`.
    pub fn to_css(&self) -> String {
        let default = self.default_scheme();
        let mut css = String::new();
        css.push_str(":root {\n");
        css.push_str(&format!("  color-scheme: {};\n", default.as_str()));
        for spec in TOKEN_REGISTRY {
            if let Some(value) = self.value(default, spec.name) {
                css.push_str(&format!("  {}: {value};\n", css_variable_name(spec.name)));
            }
        }
        css.push_str("}\n");

        if let Some(other) = self
            .colors
            .keys()
            .copied()
            .find(|scheme| *scheme != default)
        {
            css.push_str(&format!(
                "@media (prefers-color-scheme: {}) {{\n",
                other.as_str()
            ));
            self.scheme_block(&mut css, other, ":root:not([data-glint-scheme])", "  ");
            css.push_str("}\n");
            for scheme in [default, other] {
                let selector = format!(":root[data-glint-scheme=\"{}\"]", scheme.as_str());
                self.scheme_block(&mut css, scheme, &selector, "");
            }
        }

        let durations: Vec<&TokenSpec> = TOKEN_REGISTRY
            .iter()
            .filter(|spec| spec.kind == TokenKind::Duration)
            .collect();
        css.push_str("@media (prefers-reduced-motion: reduce) {\n");
        reduced_motion_block(&mut css, &durations, ":root", "  ");
        css.push_str("}\n");
        reduced_motion_block(
            &mut css,
            &durations,
            ":root[data-glint-reduced-motion=\"true\"]",
            "",
        );
        css
    }

    fn scheme_block(&self, css: &mut String, scheme: ColorScheme, selector: &str, indent: &str) {
        css.push_str(&format!("{indent}{selector} {{\n"));
        css.push_str(&format!("{indent}  color-scheme: {};\n", scheme.as_str()));
        for spec in TOKEN_REGISTRY.iter().filter(|s| s.kind == TokenKind::Color) {
            if let Some(value) = self.value(scheme, spec.name) {
                css.push_str(&format!(
                    "{indent}  {}: {value};\n",
                    css_variable_name(spec.name)
                ));
            }
        }
        css.push_str(&format!("{indent}}}\n"));
    }
}

fn reduced_motion_block(css: &mut String, durations: &[&TokenSpec], selector: &str, indent: &str) {
    css.push_str(&format!("{indent}{selector} {{\n"));
    for spec in durations {
        css.push_str(&format!(
            "{indent}  {}: 0ms;\n",
            css_variable_name(spec.name)
        ));
    }
    css.push_str(&format!("{indent}}}\n"));
}

// ------------------------------------------------------------------ parser

/// Parse and validate a `tokens.json` document.
///
/// Checks run in a fixed order: size, JSON syntax and duplicate keys, root
/// type, `schemaVersion`, unknown top-level fields, `colorSchemes` (names,
/// token names, values, completeness, contrast), then `tokens`. The parser
/// stops at the first problem.
///
/// # Errors
///
/// A [`TokenError`] with a stable [`TokenErrorCode`] and a JSON Pointer.
pub fn parse_tokens(text: &str) -> Result<TokenFile> {
    let root = load_json_object(text, MAX_TOKENS_BYTES, "tokens file")?;
    let root = Obj::root(&root);

    match root.get("schemaVersion") {
        None => {
            return Err(TokenError::new(
                TokenErrorCode::MissingSchemaVersion,
                root.child("schemaVersion"),
                "`schemaVersion` is required",
            ));
        }
        Some(value) => {
            if value.as_u64() != Some(u64::from(SUPPORTED_TOKENS_SCHEMA_VERSION)) {
                let code = if value.is_number() {
                    TokenErrorCode::UnsupportedSchemaVersion
                } else {
                    TokenErrorCode::InvalidType
                };
                return Err(TokenError::new(
                    code,
                    root.child("schemaVersion"),
                    format!("`schemaVersion` must be {SUPPORTED_TOKENS_SCHEMA_VERSION}"),
                ));
            }
        }
    }
    root.deny_unknown(TOP_LEVEL_FIELDS)?;
    for key in ["$schema", "$comment"] {
        if let Some(value) = root.get(key) {
            let text = value
                .as_str()
                .ok_or_else(|| wrong_type(value, root.child(key), "a string"))?;
            if text.chars().count() > MAX_COMMENT_LEN {
                return Err(TokenError::new(
                    TokenErrorCode::InvalidValue,
                    root.child(key),
                    format!("`{key}` is longer than {MAX_COMMENT_LEN} characters"),
                ));
            }
        }
    }

    let colors = color_schemes(&root)?;
    let values = plain_tokens(&root)?;
    Ok(TokenFile {
        schema_version: SUPPORTED_TOKENS_SCHEMA_VERSION,
        colors,
        values,
    })
}

/// Validate one value for one token of the contract, outside a `tokens.json`
/// (for example a profile `tokenOverrides` entry), and return its normalised
/// CSS text (colours lowercased, numbers in canonical form).
///
/// The same rules as [`parse_tokens`] apply: a brand-constant name is
/// [`TokenErrorCode::BrandName`], a name outside [`TOKEN_REGISTRY`] is
/// [`TokenErrorCode::UnknownToken`] and a value that does not match the
/// token's [`TokenKind`] grammar is [`TokenErrorCode::InvalidValue`] (or
/// [`TokenErrorCode::InvalidType`] for a JSON value of the wrong type).
/// `pointer` is reported unchanged in the error. Contrast is not checked
/// here: it depends on the other tokens of the scheme.
///
/// # Errors
///
/// A [`TokenError`] as described above.
pub fn validate_token_value(name: &str, value: &Value, pointer: &str) -> Result<String> {
    if let Some(err) = brand_error(name, pointer) {
        return Err(err);
    }
    let Some(spec) = token_spec(name) else {
        return Err(TokenError::new(
            TokenErrorCode::UnknownToken,
            pointer,
            format!("{name:?} is not a token of the contract"),
        ));
    };
    validate_value(spec, value, pointer)
}

fn brand_error(name: &str, pointer: &str) -> Option<TokenError> {
    is_brand_name(name).then(|| {
        TokenError::new(
            TokenErrorCode::BrandName,
            pointer,
            format!(
                "{name:?} uses a Glint brand constant; token names are semantic roles such as \
                 `background.default`"
            ),
        )
    })
}

fn color_schemes(root: &Obj<'_>) -> Result<BTreeMap<ColorScheme, BTreeMap<String, String>>> {
    let missing = || {
        TokenError::new(
            TokenErrorCode::InvalidColorScheme,
            root.child("colorSchemes"),
            "`colorSchemes` must define `dark`, `light` or both",
        )
    };
    let value = root.get("colorSchemes").ok_or_else(missing)?;
    let schemes = as_object(value, root.child("colorSchemes"))?;
    if schemes.map.is_empty() {
        return Err(missing());
    }
    let mut out = BTreeMap::new();
    for (scheme_name, scheme_value) in schemes.map {
        let pointer = schemes.child(scheme_name);
        let Some(scheme) = ColorScheme::from_name(scheme_name) else {
            return Err(TokenError::new(
                TokenErrorCode::InvalidColorScheme,
                pointer,
                format!("unknown colour scheme {scheme_name:?}; use `dark` or `light`"),
            ));
        };
        let obj = as_object(scheme_value, pointer)?;
        let mut colors = BTreeMap::new();
        for (name, value) in obj.map {
            let pointer = obj.child(name);
            if let Some(err) = brand_error(name, &pointer) {
                return Err(err);
            }
            let Some(spec) = token_spec(name) else {
                return Err(TokenError::new(
                    TokenErrorCode::UnknownToken,
                    pointer,
                    format!("{name:?} is not a token of the contract"),
                ));
            };
            if spec.kind != TokenKind::Color {
                return Err(TokenError::new(
                    TokenErrorCode::InvalidValue,
                    pointer,
                    format!("{name:?} is not a colour token; define it under `tokens`"),
                ));
            }
            colors.insert(name.clone(), validate_value(spec, value, &pointer)?);
        }
        for spec in TOKEN_REGISTRY.iter().filter(|s| s.kind == TokenKind::Color) {
            if !colors.contains_key(spec.name) {
                return Err(TokenError::new(
                    TokenErrorCode::MissingToken,
                    obj.child(spec.name),
                    format!("colour scheme {scheme_name:?} must define {:?}", spec.name),
                ));
            }
        }
        check_contrast(&obj, scheme_name, &colors)?;
        out.insert(scheme, colors);
    }
    Ok(out)
}

fn check_contrast(
    scheme: &Obj<'_>,
    scheme_name: &str,
    colors: &BTreeMap<String, String>,
) -> Result<()> {
    for req in CONTRAST_REQUIREMENTS {
        let parse = |name: &str| Rgba::parse_hex(&colors[name]).expect("validated colour");
        let (fg, bg) = (parse(req.foreground), parse(req.background));
        for (name, color) in [(req.foreground, fg), (req.background, bg)] {
            if color.a != 255 {
                return Err(TokenError::new(
                    TokenErrorCode::InvalidValue,
                    scheme.child(name),
                    format!("{name:?} takes part in a contrast requirement and must be opaque"),
                ));
            }
        }
        let ratio = contrast_ratio(fg, bg);
        if ratio < req.minimum {
            return Err(TokenError::new(
                TokenErrorCode::LowContrast,
                scheme.child(req.foreground),
                format!(
                    "{:?} on {:?} in the {scheme_name} scheme has contrast {ratio:.2}:1; \
                     the minimum is {}:1",
                    req.foreground, req.background, req.minimum
                ),
            ));
        }
    }
    Ok(())
}

fn plain_tokens(root: &Obj<'_>) -> Result<BTreeMap<String, String>> {
    let Some(value) = root.get("tokens") else {
        return Err(TokenError::new(
            TokenErrorCode::MissingToken,
            root.child("tokens"),
            "`tokens` is required",
        ));
    };
    let tokens = as_object(value, root.child("tokens"))?;
    let mut out = BTreeMap::new();
    for (name, value) in tokens.map {
        let pointer = tokens.child(name);
        if let Some(err) = brand_error(name, &pointer) {
            return Err(err);
        }
        let Some(spec) = token_spec(name) else {
            return Err(TokenError::new(
                TokenErrorCode::UnknownToken,
                pointer,
                format!("{name:?} is not a token of the contract"),
            ));
        };
        if spec.kind == TokenKind::Color {
            return Err(TokenError::new(
                TokenErrorCode::InvalidValue,
                pointer,
                format!("{name:?} is a colour token; define it under `colorSchemes`"),
            ));
        }
        out.insert(name.clone(), validate_value(spec, value, &pointer)?);
    }
    for spec in TOKEN_REGISTRY.iter().filter(|s| s.kind != TokenKind::Color) {
        if !out.contains_key(spec.name) {
            return Err(TokenError::new(
                TokenErrorCode::MissingToken,
                tokens.child(spec.name),
                format!("`tokens` must define {:?}", spec.name),
            ));
        }
    }
    Ok(out)
}

// ------------------------------------------------------------- value rules

fn bad(spec: &TokenSpec, pointer: &str, expected: &str) -> TokenError {
    TokenError::new(
        TokenErrorCode::InvalidValue,
        pointer,
        format!("{:?} must be {expected}", spec.name),
    )
}

/// Validate one value and return its normalised CSS text.
fn validate_value(spec: &TokenSpec, value: &Value, pointer: &str) -> Result<String> {
    match spec.kind {
        TokenKind::FontWeight => {
            let weight = value
                .as_u64()
                .filter(|w| (1..=1000).contains(w))
                .ok_or_else(|| bad(spec, pointer, "an integer from 1 to 1000"))?;
            return Ok(weight.to_string());
        }
        TokenKind::Number => {
            let number = finite(value)
                .filter(|n| (0.5..=3.0).contains(n))
                .ok_or_else(|| bad(spec, pointer, "a number from 0.5 to 3"))?;
            return Ok(format_number(number));
        }
        _ => {}
    }
    let text = value
        .as_str()
        .ok_or_else(|| wrong_type(value, pointer, "a string"))?;
    if text.len() > MAX_VALUE_LEN {
        return Err(bad(spec, pointer, "at most 256 bytes"));
    }
    let text = text.trim();
    let ok = match spec.kind {
        TokenKind::Color => Rgba::parse_hex(text).is_some(),
        TokenKind::Dimension => dimension(text, false),
        TokenKind::FontFamily => font_family(text),
        TokenKind::Duration => duration(text),
        TokenKind::Easing => easing(text),
        TokenKind::Shadow => shadow(text),
        TokenKind::FontWeight | TokenKind::Number => unreachable!("handled above"),
    };
    if !ok {
        return Err(bad(spec, pointer, expectation(spec.kind)));
    }
    Ok(match spec.kind {
        TokenKind::Color => text.to_ascii_lowercase(),
        _ => text.to_owned(),
    })
}

fn expectation(kind: TokenKind) -> &'static str {
    match kind {
        TokenKind::Color => "a hex colour such as `#1b1c20` (3, 4, 6 or 8 digits)",
        TokenKind::Dimension => "`0` or a non-negative number with px, rem or em",
        TokenKind::FontFamily => {
            "a comma-separated font stack of at most 8 plain or double-quoted family names \
             ending in a generic family such as `sans-serif`"
        }
        TokenKind::Duration => "a duration such as `120ms` or `0.2s`, at most 5s",
        TokenKind::Easing => {
            "`linear`, `ease`, `ease-in`, `ease-out`, `ease-in-out` or \
             `cubic-bezier(x1, y1, x2, y2)` with x1 and x2 in 0..=1"
        }
        TokenKind::Shadow => {
            "`none` or up to four comma-separated layers of `<x> <y> <blur> [<spread>] <hex colour>`"
        }
        TokenKind::FontWeight | TokenKind::Number => "a number",
    }
}

fn finite(value: &Value) -> Option<f64> {
    match value {
        Value::Number(number) => number.as_f64().filter(|n| n.is_finite()),
        _ => None,
    }
}

fn format_number(number: f64) -> String {
    Number::from_f64(number).map_or_else(|| number.to_string(), |n| n.to_string())
}

/// A plain decimal such as `12` or `0.5` (no sign, no exponent), at most 1e4.
fn plain_number(text: &str) -> Option<f64> {
    let (int, frac) = text
        .split_once('.')
        .map_or((text, None), |(i, f)| (i, Some(f)));
    let digits = |s: &str| !s.is_empty() && s.len() <= 6 && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(int) || frac.is_some_and(|f| !digits(f)) {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| *n <= 10_000.0)
}

fn dimension(text: &str, allow_negative: bool) -> bool {
    let body = if allow_negative {
        text.strip_prefix('-').unwrap_or(text)
    } else {
        text
    };
    if body == "0" {
        return true;
    }
    ["px", "rem", "em"]
        .iter()
        .find_map(|unit| body.strip_suffix(unit))
        .is_some_and(|number| plain_number(number).is_some())
}

fn duration(text: &str) -> bool {
    let (number, scale) = if let Some(n) = text.strip_suffix("ms") {
        (n, 1.0)
    } else if let Some(n) = text.strip_suffix('s') {
        (n, 1000.0)
    } else {
        return false;
    };
    plain_number(number).is_some_and(|n| n * scale <= 5000.0)
}

fn easing(text: &str) -> bool {
    if ["linear", "ease", "ease-in", "ease-out", "ease-in-out"].contains(&text) {
        return true;
    }
    let Some(args) = text
        .strip_prefix("cubic-bezier(")
        .and_then(|rest| rest.strip_suffix(')'))
    else {
        return false;
    };
    let parts: Vec<&str> = args.split(',').map(str::trim).collect();
    if parts.len() != 4 {
        return false;
    }
    let number = |s: &str| {
        let (negative, body) = s.strip_prefix('-').map_or((false, s), |b| (true, b));
        plain_number(body).map(|n| if negative { -n } else { n })
    };
    let values: Vec<Option<f64>> = parts.iter().map(|p| number(p)).collect();
    let [Some(x1), Some(y1), Some(x2), Some(y2)] = values[..] else {
        return false;
    };
    (0.0..=1.0).contains(&x1)
        && (0.0..=1.0).contains(&x2)
        && (-4.0..=4.0).contains(&y1)
        && (-4.0..=4.0).contains(&y2)
}

const GENERIC_FAMILIES: &[&str] = &[
    "serif",
    "sans-serif",
    "monospace",
    "cursive",
    "fantasy",
    "system-ui",
    "ui-serif",
    "ui-sans-serif",
    "ui-monospace",
    "ui-rounded",
];
const CSS_WIDE_KEYWORDS: &[&str] = &[
    "inherit",
    "initial",
    "unset",
    "revert",
    "revert-layer",
    "default",
];

fn font_family(text: &str) -> bool {
    let entries: Vec<&str> = text.split(',').map(str::trim).collect();
    if entries.len() > 8 {
        return false;
    }
    let valid = entries.iter().all(|entry| {
        if let Some(quoted) = entry
            .strip_prefix('"')
            .and_then(|rest| rest.strip_suffix('"'))
        {
            !quoted.trim().is_empty()
                && quoted
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b' ' || b == b'-' || b == b'_')
        } else {
            entry
                .as_bytes()
                .first()
                .is_some_and(u8::is_ascii_alphabetic)
                && entry
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'-')
                && !CSS_WIDE_KEYWORDS.contains(entry)
        }
    });
    valid
        && entries
            .last()
            .is_some_and(|last| GENERIC_FAMILIES.contains(last))
}

fn shadow(text: &str) -> bool {
    if text == "none" {
        return true;
    }
    let layers: Vec<&str> = text.split(',').map(str::trim).collect();
    if layers.len() > 4 {
        return false;
    }
    layers.iter().all(|layer| {
        let parts: Vec<&str> = layer.split_whitespace().collect();
        let Some((color, lengths)) = parts.split_last() else {
            return false;
        };
        (3..=4).contains(&lengths.len())
            // Offsets and spread may be negative; blur (index 2) may not.
            && lengths
                .iter()
                .enumerate()
                .all(|(i, part)| dimension(part, i != 2))
            && Rgba::parse_hex(color).is_some()
    })
}

/// JSON Pointer of a colour token inside a scheme: `/colorSchemes/dark/<name>`.
pub fn color_pointer(scheme: ColorScheme, name: &str) -> String {
    child_pointer(&child_pointer("/colorSchemes", scheme.as_str()), name)
}
