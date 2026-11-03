//! Native chest menus share JSON-UI cells; their server-owned row count sets geometry.
use crate::container_kinds::{Cell, ContainerKind, storage_kind};
use json_ui::Context;
use serde_json::Value;

const STORAGE_SLOTS: usize = inventory::java::GENERIC_MENU_MAX_ROWS * inventory::java::HOTBAR_SIZE;
const CELLS: [Cell; STORAGE_SLOTS] = {
    let mut cells = [Cell::Storage(0); STORAGE_SLOTS];
    let mut i = 0;
    while i < cells.len() {
        cells[i] = Cell::Storage(i as u8);
        i += 1;
    }
    cells
};
const GENERIC: ContainerKind = ContainerKind {
    screen: json_ui::NATIVE_STORAGE_SCREEN,
    title_key: "container.chest",
    collections: &[("container_items", &CELLS)],
    flags: &[],
};

/// Other station types must select their own layout and rules, never this grid.
pub fn kind(slots: usize) -> Option<&'static ContainerKind> {
    if slots == 0 || slots > CELLS.len() || !slots.is_multiple_of(9) {
        return None;
    }
    Some(if matches!(slots, 27 | 54) {
        storage_kind(slots, None)
    } else {
        &GENERIC
    })
}

/// Target ChestMenu positions: title, row grid, 3 storage rows, then hotbar.
pub(super) fn context(mut context: Context, slots: usize) -> Option<Context> {
    kind(slots)?;
    let height = (slots / 9 * 18) as u32;
    for (key, value) in [
        ("storage_rows", height / 18),
        ("storage_height", height),
        ("panel_height", height + 114),
        ("inventory_label_y", height + 18),
        ("inventory_y", height + 31),
        ("hotbar_y", height + 89),
    ] {
        context = context.with_var(key, Value::from(value));
    }
    Some(context)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::containers::{NativeCell, NativeInventory, ScreenLayout};
    struct Metrics;
    impl json_ui::TextMeasure for Metrics {
        fn extent(&self, text: &str) -> [f64; 2] {
            [text.len() as f64 * 6., 9.]
        }
    }
    impl json_ui::TextureSource for Metrics {
        fn texture(&self, _: &str) -> Option<json_ui::TextureMeta> {
            Some(json_ui::TextureMeta::plain([16., 16.]))
        }
    }
    #[test]
    fn every_row_count_has_exact_cells_and_independent_storage_player_hit_positions() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.local/assets/compiled/vanilla-v1.mcbeui");
        let Ok(bytes) = std::fs::read(&path) else {
            eprintln!(
                "SKIP native menu layout: missing UI carrier {}",
                path.display()
            );
            return;
        };
        let carrier = assets::RuntimeUiAssets::decode(&bytes).unwrap();
        let mut catalog = json_ui::Catalog::from_files(
            carrier
                .ui_files()
                .iter()
                .map(|f| (f.path.as_ref(), f.bytes.as_ref())),
        )
        .unwrap();
        catalog.apply_pack(
            crate::hud::JAVA_HUD_PACK
                .iter()
                .map(|(path, _, bytes)| (*path, *bytes)),
        );
        for rows in [1, 2, 4, 5] {
            let slots = rows * 9;
            let layout = ScreenLayout::Station(kind(slots).unwrap());
            let native = NativeInventory {
                player: vec![
                    NativeCell {
                        known: true,
                        ..Default::default()
                    };
                    46
                ],
                container: vec![
                    NativeCell {
                        known: true,
                        ..Default::default()
                    };
                    slots
                ],
                ..Default::default()
            };
            let data = native.data(layout, "Server menu", &mut Vec::new());
            let context = context(Context::desktop(), slots)
                .unwrap()
                .with_var("container_title", Value::from("Server menu"))
                .with_var("inventory_title", Value::from("Inventory"));
            let rendered = json_ui::render_screen(
                json_ui::NATIVE_STORAGE_SCREEN,
                &catalog,
                &context,
                &data,
                [720., 480.],
                &json_ui::LayoutEnv {
                    text: &Metrics,
                    textures: &Metrics,
                },
                &json_ui::ViewState::default(),
            )
            .unwrap();
            let storage: Vec<_> = rendered
                .hits
                .iter()
                .filter(|h| {
                    h.kind == json_ui::HitKind::Button
                        && h.collection.as_deref() == Some("container_items")
                })
                .collect();
            assert_eq!(storage.len(), slots, "rows={rows}");
            assert_eq!(storage.last().unwrap().collection_index, Some(slots - 1));
            let panel_top = (480. - (114. + rows as f64 * 18.)) / 2.;
            for (collection, index, x, y) in [
                ("container_items", 0, 288., panel_top + 26.),
                (
                    "container_items",
                    slots - 1,
                    432.,
                    panel_top + 26. + (rows - 1) as f64 * 18.,
                ),
                (
                    "inventory_items",
                    0,
                    288.,
                    panel_top + rows as f64 * 18. + 40.,
                ),
                ("hotbar_items", 0, 288., panel_top + rows as f64 * 18. + 98.),
            ] {
                let hit = rendered
                    .hits
                    .iter()
                    .find(|h| {
                        h.kind == json_ui::HitKind::Button
                            && h.collection.as_deref() == Some(collection)
                            && h.collection_index == Some(index)
                    })
                    .expect("addressed cell");
                assert_eq!(
                    [hit.rect.x + hit.rect.w / 2., hit.rect.y + hit.rect.h / 2.],
                    [x, y],
                    "{rows}/{collection}/{index}"
                );
            }
        }
        for bad in [0, 8, 55, 63, usize::MAX] {
            assert!(kind(bad).is_none());
        }
    }
}
