//! CAP-005: every capability the surface catalog names is in the frozen v1
//! capability registry owned by `glint-platform`, and every registered
//! `surface.*` ID belongs to a catalog kind or to custom surfaces.

use glint_platform::Namespace;
use glint_platform::registry::{self, Access};
use glint_surfaces::{CATALOG, CUSTOM_SURFACE_CAPABILITY, StandardSurface};

#[test]
fn every_catalog_capability_is_registered() {
    assert!(registry::is_registered(CUSTOM_SURFACE_CAPABILITY));
    for entry in CATALOG {
        let spec = registry::lookup(entry.capability)
            .unwrap_or_else(|| panic!("{} is not registered", entry.capability));
        assert_eq!(spec.access, Access::Present, "{}", entry.capability);
        assert_eq!(entry.capability, format!("surface.{}", entry.id));
        for id in entry.optional_capabilities {
            let spec = registry::lookup(id)
                .unwrap_or_else(|| panic!("{}: {id} is not registered", entry.id));
            assert_ne!(spec.namespace(), Namespace::Surface, "{id}");
        }
    }
}

#[test]
fn every_registered_surface_capability_is_a_catalog_kind() {
    for spec in registry::REGISTRY {
        if spec.namespace() != Namespace::Surface {
            continue;
        }
        let name = spec.id.strip_prefix("surface.").unwrap_or_default();
        assert!(
            spec.id == CUSTOM_SURFACE_CAPABILITY || StandardSurface::from_id(name).is_some(),
            "{} has no surface kind",
            spec.id
        );
    }
}
