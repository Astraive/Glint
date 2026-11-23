//! `SurfaceDefinition`: one validated surface of a theme.

use glint_platform::CapabilityId;
use glint_schema::{Surface, ThemeManifest};
use serde::Serialize;
use serde_json::{Map, Value};

use crate::behavior::{self, SurfaceBehavior};
use crate::catalog::{
    CUSTOM_PREFIX, CustomSurfaceId, MAX_SURFACE_ID_LEN, StandardSurface, SurfaceKind, is_name,
};
use crate::entrypoint;
use crate::error::{SurfaceError, SurfaceErrorCode, child_pointer};
use crate::json::{Obj, Result};
use crate::placement::{self, SurfacePlacement};

/// Every key a surface definition object (`surfaces.<id>`) accepts.
pub const DEFINITION_FIELDS: &[&str] = &["entry", "placement", "behavior"];

/// A validated surface: the ticket's
/// `SurfaceDefinition {id, kind, entrypoint, placement, behavior, capabilities}`.
///
/// In the v1 manifest a surface's ID is its key in `surfaces`, and that key
/// is also its kind (one surface per kind per theme), so `id` equals
/// `kind.as_str()` today. They are kept apart so profiles can later place
/// several instances of one kind. Serializes to the JSON shape of the
/// generated TypeScript `SurfaceDefinition` type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SurfaceDefinition {
    /// Surface ID (the manifest key).
    pub id: String,
    /// Standard or custom kind.
    pub kind: SurfaceKind,
    /// Package-relative `.html` entry document.
    pub entrypoint: String,
    /// Placement request with defaults applied.
    pub placement: SurfacePlacement,
    /// Behaviour request with defaults applied.
    pub behavior: SurfaceBehavior,
    /// Capabilities implied by the kind.
    pub capabilities: SurfaceCapabilities,
}

/// Capabilities a surface implies, from the catalog.
///
/// `required`: the host must offer every one or the surface cannot be shown
/// (the profile falls back to a default, SRF-004). `optional`: used when
/// usable; otherwise the matching controls are disabled and the surface
/// still shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[non_exhaustive]
pub struct SurfaceCapabilities {
    /// Capabilities the surface cannot be shown without.
    pub required: Vec<CapabilityId>,
    /// Capabilities the surface uses when available.
    pub optional: Vec<CapabilityId>,
}

impl SurfaceCapabilities {
    /// The capabilities implied by `kind`.
    pub fn for_kind(kind: &SurfaceKind) -> Self {
        let id = |raw: &str| CapabilityId::parse(raw).expect("catalog capability IDs are valid");
        Self {
            required: vec![id(kind.capability())],
            optional: kind
                .catalog_entry()
                .map(|entry| {
                    entry
                        .optional_capabilities
                        .iter()
                        .map(|raw| id(raw))
                        .collect()
                })
                .unwrap_or_default(),
        }
    }
}

/// Parses a surface ID (a `surfaces` key) into its kind.
///
/// # Errors
///
/// `GLINT_SURFACE_INVALID_CUSTOM_ID` for a `custom:` ID without a dotted
/// namespace (`custom:clock`), `GLINT_SURFACE_UNKNOWN_KIND` for a well-formed
/// name outside the catalog (`taskbar`) and `GLINT_SURFACE_INVALID_ID` for
/// anything else. The pointer is `/surfaces/<id>`.
pub fn parse_surface_id(id: &str) -> std::result::Result<SurfaceKind, SurfaceError> {
    let pointer = child_pointer("/surfaces", id);
    if let Some(rest) = id.strip_prefix(CUSTOM_PREFIX) {
        return CustomSurfaceId::parse(id).map(SurfaceKind::Custom).ok_or_else(|| {
            let why = if rest.contains('.') || id.len() > MAX_SURFACE_ID_LEN {
                "is malformed"
            } else {
                "has no namespace"
            };
            SurfaceError::new(
                SurfaceErrorCode::InvalidCustomId,
                pointer,
                format!(
                    "{id:?} {why}: custom surfaces are `custom:<namespace>.<name>` with at least \
                     two dot-separated lowercase segments (e.g. custom:dev.example.clock)"
                ),
            )
        });
    }
    if let Some(kind) = StandardSurface::from_id(id) {
        return Ok(SurfaceKind::Standard(kind));
    }
    if is_name(id) {
        let known: Vec<&str> = StandardSurface::ALL.iter().map(|k| k.as_str()).collect();
        return Err(SurfaceError::new(
            SurfaceErrorCode::UnknownKind,
            pointer,
            format!(
                "{id:?} is not a standard surface; use one of {} or a custom:<namespace>.<name> ID",
                known.join(", ")
            ),
        ));
    }
    Err(SurfaceError::new(
        SurfaceErrorCode::InvalidId,
        pointer,
        format!(
            "{id:?} is not a surface ID: use a lowercase standard name or custom:<namespace>.<name>"
        ),
    ))
}

impl SurfaceDefinition {
    /// Parses one surface definition object (`{entry, placement?, behavior?}`)
    /// stored under `surfaces.<id>`. Pointers in errors are relative to the
    /// manifest root (`/surfaces/<id>/...`).
    ///
    /// Checks run in a fixed order and stop at the first problem: ID, object
    /// type, unknown fields, `entry`, `placement`, `behavior`.
    ///
    /// # Errors
    ///
    /// A [`SurfaceError`] with a stable [`SurfaceErrorCode`].
    pub fn parse(id: &str, value: &Value) -> std::result::Result<Self, SurfaceError> {
        let kind = parse_surface_id(id)?;
        let obj = Obj::new(value, child_pointer("/surfaces", id))?;
        obj.deny_unknown(DEFINITION_FIELDS)?;
        let entry_ptr = obj.child("entry");
        let entrypoint = match obj.get("entry") {
            None => {
                return Err(SurfaceError::new(
                    SurfaceErrorCode::MissingField,
                    entry_ptr,
                    "a surface needs an `entry` document",
                ));
            }
            Some(Value::String(path)) => path.clone(),
            Some(other) => return Err(crate::json::type_error(&entry_ptr, "a string", other)),
        };
        entrypoint::check(&entrypoint, &entry_ptr)?;
        let placement = placement::parse(&kind, obj.get("placement"), &obj.child("placement"))?;
        let behavior = behavior::parse(
            &kind,
            &placement,
            obj.get("behavior"),
            &obj.child("behavior"),
        )?;
        let capabilities = SurfaceCapabilities::for_kind(&kind);
        Ok(Self {
            id: id.to_owned(),
            kind,
            entrypoint,
            placement,
            behavior,
            capabilities,
        })
    }

    /// Validates one surface of a manifest already accepted by
    /// `glint_schema::parse_theme_manifest`.
    ///
    /// # Errors
    ///
    /// As [`SurfaceDefinition::parse`].
    pub fn from_manifest_surface(
        id: &str,
        surface: &Surface,
    ) -> std::result::Result<Self, SurfaceError> {
        let mut object = Map::new();
        object.insert("entry".into(), Value::String(surface.entry.clone()));
        if let Some(placement) = &surface.placement {
            object.insert("placement".into(), Value::Object(placement.clone()));
        }
        if let Some(behavior) = &surface.behavior {
            object.insert("behavior".into(), Value::Object(behavior.clone()));
        }
        Self::parse(id, &Value::Object(object))
    }
}

/// Validates every `surfaces.<id>` of a parsed manifest, in key order, and
/// returns the definitions. `glint-schema` keeps `placement` and `behavior`
/// as raw JSON; this is where their shape is enforced, so a host must call it
/// before mounting any surface.
///
/// # Errors
///
/// The first [`SurfaceError`] found, with a pointer relative to the manifest
/// root.
pub fn surface_definitions(
    manifest: &ThemeManifest,
) -> std::result::Result<Vec<SurfaceDefinition>, SurfaceError> {
    manifest
        .surfaces
        .iter()
        .map(|(id, surface)| SurfaceDefinition::from_manifest_surface(id, surface))
        .collect::<Result<Vec<_>>>()
}
