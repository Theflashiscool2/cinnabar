//! Cursor-stack artwork and decorations above container screens.

use json_ui::{Draw, DrawNode, RectOut, TextAlign};
use protocol::NetworkItemStack;
use serde_json::Value;
use ui::IconRef;

/// A clip wide enough to never cut the held stack (virtual px).
const UNCLIPPED: f64 = 1.0e5;

/// The held stack drawn under the pointer, above the screen.
pub(super) fn held_stack(
    stack: Option<&NetworkItemStack>,
    icon: Option<IconRef>,
    durability: Option<f32>,
    pointer: Option<[f32; 2]>,
    icons: &mut Vec<IconRef>,
) -> Vec<DrawNode> {
    held_count(
        stack.map_or(0, |stack| u32::from(stack.count)),
        icon,
        durability,
        pointer,
        icons,
    )
}

/// Draws native or translated presentation values through the same cursor renderer.
pub(super) fn held_count(
    count: u32,
    icon: Option<IconRef>,
    durability: Option<f32>,
    pointer: Option<[f32; 2]>,
    icons: &mut Vec<IconRef>,
) -> Vec<DrawNode> {
    let Some(point) = pointer.filter(|_| count > 0) else {
        return Vec::new();
    };
    let clip = RectOut {
        x: -UNCLIPPED,
        y: -UNCLIPPED,
        w: UNCLIPPED * 2.0,
        h: UNCLIPPED * 2.0,
    };
    let (x, y) = (f64::from(point[0]) - 8.0, f64::from(point[1]) - 8.0);
    let node = |dest: RectOut, draw: Draw| DrawNode {
        name: "held_item".to_owned(),
        key: String::new(),
        dest,
        clip,
        layer: i32::MAX,
        alpha: 1.0,
        anim: None,
        paint_region: None,
        draw,
        gates: Vec::new(),
    };
    let mut nodes = Vec::new();
    if let Some(icon) = icon {
        icons.push(icon);
        nodes.push(node(
            RectOut {
                x,
                y,
                w: 16.0,
                h: 16.0,
            },
            Draw::Custom {
                renderer: "inventory_item_renderer".to_owned(),
                data: [(
                    "#item_renderer_data".to_owned(),
                    Value::from((icons.len() - 1) as u64),
                )]
                .into_iter()
                .collect(),
            },
        ));
    }
    if let Some(fraction) = durability {
        nodes.push(node(
            RectOut {
                x: x + 2.0,
                y: y + 13.0,
                w: 12.0,
                h: 1.0,
            },
            Draw::Custom {
                renderer: "progress_bar_renderer".to_owned(),
                data: [
                    ("is_durability".to_owned(), Value::Bool(true)),
                    ("drop_shadow".to_owned(), Value::Bool(true)),
                    ("round_value".to_owned(), Value::Bool(true)),
                    ("#touch_progress_bar_visible".to_owned(), Value::Bool(true)),
                    ("#progress_bar_total_amount".to_owned(), Value::from(1.0)),
                    (
                        "#progress_bar_current_amount".to_owned(),
                        Value::from(fraction),
                    ),
                ]
                .into_iter()
                .collect(),
            },
        ));
    }
    if count > 1 {
        nodes.push(node(
            RectOut {
                x,
                y: y + 8.0,
                w: 17.0,
                h: 9.0,
            },
            Draw::Text {
                text: count.to_string(),
                color: [255; 4],
                shadow: true,
                align: TextAlign::Right,
                scale: 1.0,
                localize: false,
                options: Default::default(),
            },
        ));
    }
    nodes
}
