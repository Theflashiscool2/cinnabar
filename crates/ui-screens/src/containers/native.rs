//! Edition-neutral display cells for the existing JSON-UI container renderer.
use super::*;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeCell {
    pub known: bool,
    pub count: u32,
    pub icon: Option<IconRef>,
    pub durability: Option<f32>,
    pub name: String,
}
#[derive(Clone, Debug, Default, PartialEq)]
pub struct NativeInventory {
    /// InventoryMenu order, including crafting, armor, main storage, hotbar and offhand.
    pub player: Vec<NativeCell>,
    pub container: Vec<NativeCell>,
    pub cursor: NativeCell,
}
impl NativeCell {
    fn cell(&self, icons: &mut Vec<IconRef>) -> CollectionItem {
        let data = self.icon.map_or(Value::Null, |icon| {
            let index = icons.len();
            icons.push(icon);
            Value::from(index)
        });
        CollectionItem::default()
            .with("#item_renderer_data", Scalar::Json(data))
            .with(
                "#inventory_stack_count",
                Scalar::Text(if self.count > 1 {
                    self.count.to_string()
                } else {
                    String::new()
                }),
            )
            .with("#hover_text", Scalar::Text(self.name.clone()))
            // Numeric legacy item IDs are not applicable to native Java stacks.
            .with("#item_id_aux", Scalar::Int(-1))
            .with("#java_slot_known", Scalar::Bool(self.known))
            .with("#is_selected_slot", Scalar::Bool(false))
            .with("#container_item_background", Scalar::Int(0))
            .with("#container_item_modifier", Scalar::Int(0))
            .with(
                "#item_durability_visible",
                Scalar::Bool(self.durability.is_some()),
            )
            .with("#item_durability_total_amount", Scalar::Num(1000.0))
            .with(
                "#item_durability_current_amount",
                Scalar::Num(f64::from(self.durability.unwrap_or(1.0)) * 1000.0),
            )
    }
}
impl NativeInventory {
    pub(super) fn data(
        &self,
        layout: ScreenLayout,
        title: &str,
        icons: &mut Vec<IconRef>,
    ) -> DataSource {
        let mut data = DataSource::new();
        data.set_strict(true);
        survival_globals(&mut data, title);
        let empty = NativeCell::default();
        let player = |index: usize| self.player.get(index).unwrap_or(&empty);
        data.set_collection(
            "inventory_items",
            (9..36).map(|i| player(i).cell(icons)).collect(),
        );
        data.set_collection(
            "hotbar_items",
            (36..45).map(|i| player(i).cell(icons)).collect(),
        );
        match layout {
            ScreenLayout::Personal { .. } => {
                data.set_collection(
                    "crafting_input_items",
                    (1..5).map(|i| player(i).cell(icons)).collect(),
                );
                data.set_collection("crafting_output_items", vec![player(0).cell(icons)]);
                data.set_collection(
                    "armor_items",
                    (5..9)
                        .map(|i| {
                            player(i).cell(icons).with(
                                "#empty_armor_image_visible",
                                Scalar::Bool(player(i).count == 0),
                            )
                        })
                        .collect(),
                );
                data.set_collection(
                    "offhand_items",
                    vec![player(45).cell(icons).with(
                        "#empty_offhand_image_visible",
                        Scalar::Bool(player(45).count == 0),
                    )],
                );
            }
            ScreenLayout::Station(kind) => {
                for (collection, addressed) in kind.collections {
                    let items = addressed
                        .iter()
                        .filter_map(|cell| match cell {
                            Cell::Storage(index) => self.container.get(usize::from(*index)),
                            _ => None,
                        })
                        .map(|cell| cell.cell(icons))
                        .collect();
                    data.set_collection(*collection, items);
                }
            }
            _ => {}
        }
        data
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_cells_keep_counts_without_art_and_use_no_legacy_numeric_item_id() {
        let mut icons = Vec::new();
        let cell = NativeCell {
            known: true,
            count: 37,
            name: "Named item".into(),
            ..Default::default()
        }
        .cell(&mut icons);
        assert_eq!(
            cell.values["#inventory_stack_count"],
            Scalar::Text("37".into())
        );
        assert_eq!(
            cell.values["#item_renderer_data"],
            Scalar::Json(Value::Null)
        );
        assert_eq!(cell.values["#item_id_aux"], Scalar::Int(-1));
        assert_eq!(
            cell.values["#hover_text"],
            Scalar::Text("Named item".into())
        );
        assert!(icons.is_empty());
        let icon = IconRef {
            page: 2,
            uv: [0, 0, 16, 16],
            glint: true,
            color_mask: None,
        };
        let drawn = NativeCell {
            known: true,
            count: 1,
            icon: Some(icon),
            ..Default::default()
        }
        .cell(&mut icons);
        assert_eq!(
            drawn.values["#item_renderer_data"],
            Scalar::Json(Value::from(0))
        );
        assert_eq!(icons, [icon]);
        let empty = NativeCell::default().cell(&mut icons);
        assert_eq!(empty.values["#java_slot_known"], Scalar::Bool(false));
        assert_eq!(
            empty.values["#item_renderer_data"],
            Scalar::Json(Value::Null)
        );
    }
}
