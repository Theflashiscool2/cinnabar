//! Rig id ranges: which catalog and kind of geometry an [`EntityRigId`] names.
use assets::RuntimeEntityAssets;

use super::{ActorRigGeometry, EntityRigId, geometry_from_geometry_index};

pub const DIAGNOSTIC_RIG_ID: EntityRigId = EntityRigId(u32::MAX);
const EQUIPMENT_RIG_ID_BASE: u32 = 0x8000_0000;
const ITEM_MESH_RIG_ID_BASE: u32 = 0xC000_0000;
const SKIN_RIG_ID_BASE: u32 = 0xE000_0000;

/// Number of generated item mesh ids below the reserved cosmetic and skin ranges.
pub const ITEM_MESH_RIG_ID_CAPACITY: usize =
    (SKIN_RIG_ID_BASE - ITEM_MESH_RIG_ID_BASE) as usize - cosmetics::MAX_ITEMS;

/// Rig id of an entity-catalog geometry registered as equipment geometry.
#[must_use]
pub const fn equipment_rig_id(geometry_index: u32) -> EntityRigId {
    EntityRigId(EQUIPMENT_RIG_ID_BASE + geometry_index)
}

/// Rig id of a geometry binding in the session's server-pack entity catalog.
#[must_use]
pub const fn pack_rig_id(binding_index: u32) -> EntityRigId {
    EntityRigId(assets::PACK_RIG_ID_BASE + binding_index)
}

/// Equipment rig id of a geometry in the session's server-pack catalog.
#[must_use]
pub const fn pack_equipment_rig_id(geometry_index: u32) -> EntityRigId {
    equipment_rig_id(assets::PACK_EQUIPMENT_INDEX_BASE + geometry_index)
}

/// Offset, inside the vanilla and the pack entity ranges, of render-controller layer geometry.
const LAYER_GEOMETRY_ID_OFFSET: u32 = 0x2000_0000;

/// Rig id of catalog geometry `geometry` drawn by a render controller of `body`'s entity.
#[must_use]
pub fn layer_geometry_rig_id(body: EntityRigId, geometry: u32) -> EntityRigId {
    let base = if is_pack_rig_id(body) {
        assets::PACK_RIG_ID_BASE
    } else {
        0
    };
    EntityRigId(base + LAYER_GEOMETRY_ID_OFFSET + geometry)
}

pub fn is_layer_geometry_rig_id(id: EntityRigId) -> bool {
    let local = if is_pack_rig_id(id) {
        id.0 - assets::PACK_RIG_ID_BASE
    } else {
        id.0
    };
    (LAYER_GEOMETRY_ID_OFFSET..assets::PACK_RIG_ID_BASE).contains(&local)
}

/// Every geometry the catalog's render controllers can draw, under layer ids of `body`'s range.
pub fn layer_geometries(assets: &RuntimeEntityAssets, body: EntityRigId) -> Vec<ActorRigGeometry> {
    let mut indices: Vec<u32> = assets
        .render_data()
        .geometries
        .iter()
        .map(|choice| choice.geometry)
        .collect();
    indices.sort_unstable();
    indices.dedup();
    indices
        .into_iter()
        .filter_map(|index| {
            geometry_from_geometry_index(assets, index as usize, layer_geometry_rig_id(body, index))
                .ok()
        })
        .collect()
}

pub fn is_pack_equipment_rig_id(id: EntityRigId) -> bool {
    (EQUIPMENT_RIG_ID_BASE + assets::PACK_EQUIPMENT_INDEX_BASE..ITEM_MESH_RIG_ID_BASE)
        .contains(&id.0)
}

pub fn is_pack_rig_id(id: EntityRigId) -> bool {
    (assets::PACK_RIG_ID_BASE..EQUIPMENT_RIG_ID_BASE).contains(&id.0)
}

pub fn is_equipment_rig_id(id: EntityRigId) -> bool {
    id.0 >= EQUIPMENT_RIG_ID_BASE && id != DIAGNOSTIC_RIG_ID
}

/// Rig id of an admitted cosmetic, descending from the player skin range.
#[must_use]
pub const fn cosmetic_rig_id(item_index: u16) -> EntityRigId {
    EntityRigId(SKIN_RIG_ID_BASE - 1 - item_index as u32)
}

/// Body and animated skin models for every selected player, plus the local first-person hand.
pub const MAX_SKIN_RIGS: usize =
    crate::MAX_RENDERED_PLAYERS * (1 + render_api::MAX_SKIN_ANIMATION_LAYERS) + 1;

/// Rig id of a player skin's own model in cache slot `slot`.
#[must_use]
pub const fn skin_rig_id(slot: u32) -> EntityRigId {
    EntityRigId(SKIN_RIG_ID_BASE + slot)
}

/// Whether `id` is a player skin's own model, one of the [`MAX_SKIN_RIGS`] slots
/// [`skin_rig_id`] allocates.
#[must_use]
pub fn is_skin_rig_id(id: EntityRigId) -> bool {
    (SKIN_RIG_ID_BASE..SKIN_RIG_ID_BASE + MAX_SKIN_RIGS as u32).contains(&id.0)
}

/// Rig id of a generated item mesh registered with [`ActorRigFrameBuilder::insert_geometry`].
#[must_use]
pub const fn item_mesh_rig_id(mesh_index: u32) -> EntityRigId {
    EntityRigId(ITEM_MESH_RIG_ID_BASE + mesh_index)
}

/// Isolated geometry namespace for native Java models; these are GPU IDs, never protocol IDs.
pub fn java_model_rig_id(model_index: u32) -> Option<EntityRigId> {
    const BASE: u32 = 0x1000_0000;
    const _: () = assert!(assets::MAX_ENTITY_RIG_BINDINGS < BASE as usize);
    (model_index < BASE).then(|| EntityRigId(BASE + model_index))
}

/// Native equipment occupies the gap between bounded built-in equipment and
/// pack equipment. Model indices below 65536 are fixed native armor factories;
/// higher indices are replaceable meshes for the current equipment snapshot.
pub fn java_equipment_rig_id(index: u32) -> Option<EntityRigId> {
    const _: () = assert!(assets::MAX_ENTITY_RIG_BINDINGS < 0x0800_0000);
    const _: () = assert!(assets::PACK_EQUIPMENT_INDEX_BASE >= 0x1000_0000);
    (index < 0x0800_0000).then(|| EntityRigId(0x8800_0000 + index))
}
pub fn is_java_item_mesh(id: EntityRigId) -> bool {
    (0x8801_0000..0x9000_0000).contains(&id.0)
}

#[cfg(test)]
mod native_equipment_tests {
    use super::*;
    #[test]
    fn native_equipment_namespace_never_retires_body_or_pack_or_builtin_meshes() {
        for index in [0, 42, 65535, 65536, 65537, 0x07ffffff] {
            let id = java_equipment_rig_id(index).unwrap();
            assert!(is_equipment_rig_id(id));
            assert!(!is_pack_equipment_rig_id(id));
            assert_eq!(is_java_item_mesh(id), index >= 65536);
        }
        for id in [
            java_model_rig_id(42).unwrap(),
            EntityRigId(EQUIPMENT_RIG_ID_BASE),
            EntityRigId(0x90000000),
            item_mesh_rig_id(0),
            skin_rig_id(0),
        ] {
            assert!(!is_java_item_mesh(id));
        }
        assert!(java_equipment_rig_id(0x08000000).is_none());
    }
}
