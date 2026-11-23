//! TypeScript types generated from the catalog and vocabularies in this
//! crate, so the SDK and the host never disagree on surface IDs, placement
//! words or error codes.
//!
//! `tests/typegen.rs` fails when [`TYPESCRIPT_PATH`] differs from
//! [`typescript_module`]; `GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces
//! --test typegen` rewrites it.

use std::fmt::Write as _;

use crate::behavior::{
    BEHAVIOR_FIELDS, DismissTrigger, FocusPolicy, MAX_AUTO_HIDE_MS, MIN_AUTO_HIDE_MS,
};
use crate::catalog::{CATALOG, CUSTOM_SURFACE_CAPABILITY, MAX_SURFACE_ID_LEN, StandardSurface};
use crate::entrypoint::MAX_ENTRYPOINT_LEN;
use crate::error::SurfaceErrorCode;
use crate::placement::{
    Anchor, MAX_LOGICAL_PX, MAX_MARGIN_PX, MonitorTarget, PLACEMENT_FIELDS, PlacementKind,
};

/// Repository-relative path of the generated module.
pub const TYPESCRIPT_PATH: &str = "packages/types/src/surfaces.ts";

/// Field of a generated interface: JSON key, TypeScript type, optional.
pub(crate) type Field = (&'static str, &'static str, bool);

/// JSON shape of a serialized [`crate::SurfacePlacement`].
pub(crate) const RESOLVED_PLACEMENT: &[Field] = &[
    ("kind", "PlacementKind", false),
    ("anchor", "Anchor", true),
    ("monitor", "MonitorTarget", false),
    ("width", "number", true),
    ("height", "number", true),
    ("minWidth", "number", true),
    ("maxWidth", "number", true),
    ("minHeight", "number", true),
    ("maxHeight", "number", true),
    ("margin", "number", false),
];

/// JSON shape of a serialized [`crate::SurfaceBehavior`].
pub(crate) const RESOLVED_BEHAVIOR: &[Field] = &[
    ("focus", "FocusPolicy", false),
    ("reserveWorkArea", "boolean", false),
    ("dismissOn", "DismissTrigger[]", false),
    ("autoHideMs", "number", true),
    ("clickThrough", "boolean", false),
];

/// JSON shape of a serialized [`crate::SurfaceDefinition`].
pub(crate) const RESOLVED_DEFINITION: &[Field] = &[
    ("id", "string", false),
    ("kind", "SurfaceKind", false),
    ("entrypoint", "string", false),
    ("placement", "SurfacePlacement", false),
    ("behavior", "SurfaceBehavior", false),
    ("capabilities", "SurfaceCapabilities", false),
];

fn input_type(field: &str) -> &'static str {
    match field {
        "kind" => "PlacementKind",
        "anchor" => "Anchor",
        "monitor" => "MonitorTarget",
        "width" | "height" | "minWidth" | "maxWidth" | "minHeight" | "maxHeight" | "margin"
        | "autoHideMs" => "number",
        "focus" => "FocusPolicy",
        "reserveWorkArea" | "clickThrough" => "boolean",
        "dismissOn" => "DismissTrigger[]",
        other => panic!("typegen has no TypeScript type for field {other:?}"),
    }
}

fn quote(text: &str) -> String {
    serde_json::to_string(text).expect("strings serialize")
}

fn key(text: &str) -> String {
    let plain = text.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
        && !text.as_bytes()[0].is_ascii_digit();
    if plain { text.to_owned() } else { quote(text) }
}

fn string_list(words: &[&str]) -> String {
    let items: Vec<String> = words.iter().map(|w| quote(w)).collect();
    format!("[{}]", items.join(", "))
}

fn const_union(out: &mut String, doc: &str, constant: &str, type_name: &str, words: &[&str]) {
    writeln!(out, "/** {doc} */").unwrap();
    writeln!(out, "export const {constant} = [").unwrap();
    for word in words {
        writeln!(out, "  {},", quote(word)).unwrap();
    }
    writeln!(out, "] as const;").unwrap();
    writeln!(
        out,
        "export type {type_name} = (typeof {constant})[number];\n"
    )
    .unwrap();
}

fn interface(out: &mut String, doc: &str, name: &str, fields: &[Field]) {
    writeln!(out, "/** {doc} */").unwrap();
    writeln!(out, "export interface {name} {{").unwrap();
    for (field, ty, optional) in fields {
        let mark = if *optional { "?" } else { "" };
        writeln!(out, "  {}{mark}: {ty};", key(field)).unwrap();
    }
    writeln!(out, "}}\n").unwrap();
}

fn input_fields(fields: &'static [&'static str]) -> Vec<Field> {
    fields.iter().map(|f| (*f, input_type(f), true)).collect()
}

/// The full text of [`TYPESCRIPT_PATH`].
pub fn typescript_module() -> String {
    let mut out = String::new();
    out.push_str(
        "// GENERATED FILE - DO NOT EDIT.\n\
         // Source: crates/glint-surfaces (catalog, placement, behavior, error codes). PROPOSED format.\n\
         // Regenerate: GLINT_UPDATE_TYPES=1 cargo test -p glint-surfaces --test typegen\n\
         // `cargo test` fails when this file differs from the generator output.\n\n\
         import type { CapabilityId } from \"./capabilities.ts\";\n\n",
    );
    writeln!(
        out,
        "/** Stability label of the surface definition format. */\nexport const SURFACE_FORMAT_STATUS = {};\n",
        quote(crate::FORMAT_STATUS)
    )
    .unwrap();

    let kinds: Vec<&str> = StandardSurface::ALL.iter().map(|k| k.as_str()).collect();
    const_union(
        &mut out,
        "The standard surface kinds, in catalog order.",
        "STANDARD_SURFACE_KINDS",
        "StandardSurfaceKind",
        &kinds,
    );
    out.push_str(
        "/** Author-defined surface ID: `custom:<namespace>.<name>` (at least two dot-separated segments). */\n\
         export type CustomSurfaceId = `custom:${string}.${string}`;\n\n\
         /** A surface ID: a standard kind or a custom ID. */\n\
         export type SurfaceKind = StandardSurfaceKind | CustomSurfaceId;\n\n",
    );

    let placements: Vec<&str> = PlacementKind::ALL.iter().map(|p| p.as_str()).collect();
    const_union(
        &mut out,
        "Placement kinds.",
        "PLACEMENT_KINDS",
        "PlacementKind",
        &placements,
    );
    let anchors: Vec<&str> = Anchor::ALL.iter().map(|a| a.as_str()).collect();
    const_union(
        &mut out,
        "Nine-point anchors on the target monitor's work area.",
        "ANCHORS",
        "Anchor",
        &anchors,
    );
    let edges: Vec<&str> = Anchor::EDGES.iter().map(|a| a.as_str()).collect();
    const_union(
        &mut out,
        "Anchors an `edge` placement accepts.",
        "EDGE_ANCHORS",
        "EdgeAnchor",
        &edges,
    );
    let monitors: Vec<&str> = MonitorTarget::ALL.iter().map(|m| m.as_str()).collect();
    const_union(
        &mut out,
        "Monitor targets.",
        "MONITOR_TARGETS",
        "MonitorTarget",
        &monitors,
    );
    let focus: Vec<&str> = FocusPolicy::ALL.iter().map(|f| f.as_str()).collect();
    const_union(
        &mut out,
        "Keyboard focus policies.",
        "FOCUS_POLICIES",
        "FocusPolicy",
        &focus,
    );
    let dismiss: Vec<&str> = DismissTrigger::ALL.iter().map(|d| d.as_str()).collect();
    const_union(
        &mut out,
        "Dismissal triggers.",
        "DISMISS_TRIGGERS",
        "DismissTrigger",
        &dismiss,
    );

    writeln!(
        out,
        "/** Numeric limits enforced by the parser. Sizes are logical pixels. */"
    )
    .unwrap();
    writeln!(out, "export const SURFACE_LIMITS = {{").unwrap();
    for (name, value) in [
        ("maxLogicalPx", MAX_LOGICAL_PX as usize),
        ("maxMarginPx", MAX_MARGIN_PX as usize),
        ("minAutoHideMs", MIN_AUTO_HIDE_MS as usize),
        ("maxAutoHideMs", MAX_AUTO_HIDE_MS as usize),
        ("maxEntrypointLength", MAX_ENTRYPOINT_LEN),
        ("maxSurfaceIdLength", MAX_SURFACE_ID_LEN),
    ] {
        writeln!(out, "  {name}: {value},").unwrap();
    }
    writeln!(out, "}} as const;\n").unwrap();

    interface(
        &mut out,
        "`surfaces.<id>.placement` as written in glint.json.",
        "SurfacePlacementInput",
        &input_fields(PLACEMENT_FIELDS),
    );
    interface(
        &mut out,
        "`surfaces.<id>.behavior` as written in glint.json.",
        "SurfaceBehaviorInput",
        &input_fields(BEHAVIOR_FIELDS),
    );
    interface(
        &mut out,
        "`surfaces.<id>` as written in glint.json.",
        "SurfaceEntryInput",
        &[
            ("entry", "string", false),
            ("placement", "SurfacePlacementInput", true),
            ("behavior", "SurfaceBehaviorInput", true),
        ],
    );
    interface(
        &mut out,
        "A validated placement with defaults applied.",
        "SurfacePlacement",
        RESOLVED_PLACEMENT,
    );
    interface(
        &mut out,
        "A validated behaviour with defaults applied.",
        "SurfaceBehavior",
        RESOLVED_BEHAVIOR,
    );
    interface(
        &mut out,
        "Capabilities a surface implies: `required` must all be usable to show it; `optional` only enable controls.",
        "SurfaceCapabilities",
        &[
            ("required", "CapabilityId[]", false),
            ("optional", "CapabilityId[]", false),
        ],
    );
    interface(
        &mut out,
        "A validated surface, as the host serializes it.",
        "SurfaceDefinition",
        RESOLVED_DEFINITION,
    );

    interface(
        &mut out,
        "One row of the standard surface catalog. The first placement is the default.",
        "SurfaceCatalogEntry",
        &[
            ("capability", "CapabilityId", false),
            ("optionalCapabilities", "readonly CapabilityId[]", false),
            ("placements", "readonly PlacementKind[]", false),
            ("defaultAnchor", "Anchor | null", false),
            ("defaultFocus", "FocusPolicy", false),
            ("defaultDismiss", "readonly DismissTrigger[]", false),
            ("defaultAutoHideMs", "number | null", false),
            ("defaultThickness", "number | null", false),
            ("summary", "string", false),
        ],
    );
    writeln!(
        out,
        "/** Capability every custom surface requires. */\nexport const CUSTOM_SURFACE_CAPABILITY = {};\n",
        quote(CUSTOM_SURFACE_CAPABILITY)
    )
    .unwrap();
    writeln!(out, "/** The standard surface catalog. */").unwrap();
    writeln!(out, "export const SURFACE_CATALOG = {{").unwrap();
    for entry in CATALOG {
        writeln!(out, "  {}: {{", key(entry.id)).unwrap();
        writeln!(out, "    capability: {},", quote(entry.capability)).unwrap();
        writeln!(
            out,
            "    optionalCapabilities: {},",
            string_list(entry.optional_capabilities)
        )
        .unwrap();
        let placements: Vec<&str> = entry.placements.iter().map(|p| p.as_str()).collect();
        writeln!(out, "    placements: {},", string_list(&placements)).unwrap();
        let anchor = entry
            .default_anchor
            .map_or("null".to_owned(), |a| quote(a.as_str()));
        writeln!(out, "    defaultAnchor: {anchor},").unwrap();
        writeln!(
            out,
            "    defaultFocus: {},",
            quote(entry.default_focus.as_str())
        )
        .unwrap();
        let dismiss: Vec<&str> = entry.default_dismiss.iter().map(|d| d.as_str()).collect();
        writeln!(out, "    defaultDismiss: {},", string_list(&dismiss)).unwrap();
        let hide = entry
            .default_auto_hide_ms
            .map_or("null".to_owned(), |ms| ms.to_string());
        writeln!(out, "    defaultAutoHideMs: {hide},").unwrap();
        let thickness = entry
            .default_thickness
            .map_or("null".to_owned(), |px| px.to_string());
        writeln!(out, "    defaultThickness: {thickness},").unwrap();
        writeln!(out, "    summary: {},", quote(entry.summary)).unwrap();
        writeln!(out, "  }},").unwrap();
    }
    writeln!(
        out,
        "}} as const satisfies Record<StandardSurfaceKind, SurfaceCatalogEntry>;\n"
    )
    .unwrap();

    let codes: Vec<&str> = SurfaceErrorCode::ALL.iter().map(|c| c.as_str()).collect();
    const_union(
        &mut out,
        "Stable surface definition error codes.",
        "SURFACE_ERROR_CODES",
        "SurfaceErrorCode",
        &codes,
    );
    // One trailing newline.
    while out.ends_with("\n\n") {
        out.pop();
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Value, json};

    use super::{Field, RESOLVED_BEHAVIOR, RESOLVED_DEFINITION, RESOLVED_PLACEMENT};
    use crate::SurfaceDefinition;

    /// Serialized keys must be declared, and non-optional declared keys must
    /// always be present, so the generated interfaces describe the real JSON.
    fn check(value: &Value, fields: &[Field]) {
        let keys: BTreeSet<&str> = value
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect();
        let declared: BTreeSet<&str> = fields.iter().map(|f| f.0).collect();
        assert!(keys.is_subset(&declared), "{keys:?} vs {declared:?}");
        for (key, _, optional) in fields {
            assert!(*optional || keys.contains(key), "{key} missing in {value}");
        }
    }

    #[test]
    fn generated_interfaces_match_the_serialized_definition() {
        let full = json!({
            "entry": "a.html",
            "placement": { "kind": "overlay", "anchor": "top", "width": 10, "height": 10,
                "minWidth": 1, "maxWidth": 20, "minHeight": 1, "maxHeight": 20, "margin": 2 },
            "behavior": { "focus": "none", "autoHideMs": 1000, "clickThrough": true,
                "dismissOn": ["escape"] }
        });
        let sparse = json!({ "entry": "a.html", "placement": { "monitor": "all" } });
        for (id, value) in [("notification", full), ("desktop", sparse)] {
            let definition = SurfaceDefinition::parse(id, &value).expect("valid");
            let json = serde_json::to_value(&definition).unwrap();
            check(&json, RESOLVED_DEFINITION);
            check(&json["placement"], RESOLVED_PLACEMENT);
            check(&json["behavior"], RESOLVED_BEHAVIOR);
        }
    }
}
