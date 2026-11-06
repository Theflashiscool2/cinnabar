//! CPU resource/material preparation on a single worker, with latest-request publication fence.
use super::*;
mod item;
use ::assets::registry::java::{
    equipment,
    items::{self, HeadTextures, View},
    source::Jar,
};
use render_model::{EquipmentRaster, actor::java_equipment_rig_id, equipment::java_item};
use std::collections::BTreeSet;
use std::path::PathBuf;
#[derive(Clone)]
enum Piece {
    Armor(equipment::Armor),
    Hand(java_item::Prepared),
}
fn stack(s: &inventory::java::Stack) -> Stack {
    Stack {
        item: s.item.clone(),
        count: s.count,
        added: s.added.clone(),
        removed: s.removed.clone(),
    }
}
pub(super) fn run(
    catalog: Arc<Catalog>,
    shared: Arc<(Mutex<Shared>, Condvar)>,
    models: BTreeMap<String, Arc<render_model::actor::java_geometry::Model>>,
    base: render_model::ActorArtworkPages,
    root: Result<PathBuf, String>,
) {
    let mut jar = root.and_then(|root| {
        Jar::open(
            &root.join("client.jar"),
            java_protocol::target::CLIENT_ASSETS_SHA256,
            java_protocol::target::CLIENT_ASSETS_BYTES,
        )
        .map_err(|e| e.to_string())
    });
    let mut cache = BTreeMap::<String, Result<Option<Piece>, String>>::new();
    loop {
        let request = {
            let mut w = shared.0.lock().unwrap();
            while !w.closed && w.request.is_none() {
                w = shared.1.wait(w).unwrap()
            }
            if w.closed {
                return;
            }
            w.request.take().unwrap()
        };
        let result = match &mut jar {
            Ok(jar) => prepare(&catalog, jar, &models, &base, &request, &mut cache, &shared),
            Err(e) => Err(e.clone()),
        };
        if result.is_err() {
            cache.clear();
        }
        let mut w = shared.0.lock().unwrap();
        if w.closed {
            return;
        }
        if w.desired == Some(request.revision) {
            w.result = Some((request.revision, result));
        }
    }
}
fn prepare(
    catalog: &Catalog,
    jar: &mut Jar,
    models: &BTreeMap<String, Arc<render_model::actor::java_geometry::Model>>,
    base: &render_model::ActorArtworkPages,
    request: &Request,
    cache: &mut BTreeMap<String, Result<Option<Piece>, String>>,
    shared: &Arc<(Mutex<Shared>, Condvar)>,
) -> Result<Output, String> {
    let mut output = Output {
        entries: BTreeMap::new(),
        artwork: base.clone(),
        geometries: Vec::new(),
        errors: Vec::new(),
    };
    let mut rasters = Vec::new();
    let mut indices = Vec::new();
    let mut used = BTreeSet::new();
    let mut pixels = 0usize;
    let mut vertices = 0usize;
    for player in &request.players {
        {
            let w = shared.0.lock().unwrap();
            if w.closed || w.desired != Some(request.revision) {
                return Err("native equipment superseded".into());
            }
        }
        let mut draws = Vec::new();
        for (slot, s) in player.slots.iter().enumerate().take(6) {
            let Some(s) = s else { continue };
            let s = stack(s);
            let left = (slot == 0) == player.signature.left;
            let key = format!("{}:{slot}:{}:{left}", s.key(), player.signature.slim);
            used.insert(key.clone());
            let piece = cache.entry(key).or_insert_with(|| {
                if slot < 2 {
                    catalog
                        .geometry_in(
                            &s,
                            &HeadTextures::default(),
                            if left {
                                View::ThirdPersonLeft
                            } else {
                                View::ThirdPersonRight
                            },
                        )
                        .map_err(|e| e.to_string())
                        .and_then(|q| {
                            java_item::prepare_with_ids(q, |i| {
                                java_equipment_rig_id(65536 + i as u32)
                            })
                        })
                        .map(|p| p.map(Piece::Hand))
                } else {
                    equipment::armor(jar, &s, slot as u8, player.signature.slim)
                        .map(|p| p.map(Piece::Armor))
                }
            });
            let piece = match piece {
                Ok(Some(piece)) => piece,
                Ok(None) => {
                    note(
                        &mut output.errors,
                        format!("{} slot{slot}: unsupported equipment layer", s.item),
                    );
                    continue;
                }
                Err(e) => {
                    note(&mut output.errors, format!("{} slot{slot}: {e}", s.item));
                    continue;
                }
            };
            if slot >= 2 && equipment::component(&s, "minecraft:trim").is_some() {
                note(
                    &mut output.errors,
                    format!("{}: armor trim layer not yet implemented", s.item),
                );
            }
            let glint = items::glint(&s.item, &s.added, &s.removed);
            match piece {
                Piece::Armor(armor) => {
                    let model = models
                        .get(&armor.model)
                        .ok_or_else(|| format!("missing native armor factory {}", armor.model))?;
                    for image in &armor.layers {
                        charge(
                            &mut pixels,
                            image.rgba.len(),
                            render_model::MAX_ACTOR_GPU_PIXEL_BYTES,
                        )?;
                        let raster = EquipmentRaster {
                            width: u16::try_from(image.width).map_err(|e| e.to_string())?,
                            height: u16::try_from(image.height).map_err(|e| e.to_string())?,
                            rgba8: image.rgba.clone().into(),
                        };
                        indices.push((player.id, draws.len()));
                        rasters.push(raster);
                        draws.push(Draw {
                            model: Some(model.clone()),
                            rig: model.geometry.id,
                            hand: None,
                            bounds: model.bounds,
                            artwork: placeholder(),
                            alpha: render_model::HandItemAlphaMode::Cutout,
                            glint,
                        });
                    }
                }
                Piece::Hand(hand) => {
                    item::Writer {
                        output: &mut output,
                        rasters: &mut rasters,
                        indices: &mut indices,
                        pixels: &mut pixels,
                        vertices: &mut vertices,
                    }
                    .append(player.id, &mut draws, hand, Some(left), glint)?;
                }
            }
            if draws.len() >= usize::from(render_model::ACTOR_LAYER_SKIN_BASE) {
                return Err("native player equipment layer budget exhausted".into());
            }
        }
        output.entries.insert(
            player.id,
            Entry {
                signature: player.signature,
                ground: false,
                seed: 0,
                draws,
            },
        );
    }
    for ground in &request.grounds {
        {
            let w = shared.0.lock().unwrap();
            if w.closed || w.desired != Some(request.revision) {
                return Err("stale native world item work".into());
            }
        }
        let s = stack(&ground.stack);
        let key = format!("ground:{}", s.key());
        used.insert(key.clone());
        let piece = cache.entry(key).or_insert_with(|| {
            catalog
                .geometry_in(&s, &HeadTextures::default(), View::Ground)
                .map_err(|e| e.to_string())
                .and_then(|quads| {
                    java_item::prepare_with_ids(quads, |i| java_equipment_rig_id(65536 + i as u32))
                })
                .map(|p| p.map(Piece::Hand))
        });
        let mut draws = Vec::new();
        match piece {
            Ok(Some(Piece::Hand(mesh))) => item::Writer {
                output: &mut output,
                rasters: &mut rasters,
                indices: &mut indices,
                pixels: &mut pixels,
                vertices: &mut vertices,
            }
            .append(
                ground.id,
                &mut draws,
                mesh,
                None,
                items::glint(&s.item, &s.added, &s.removed),
            )?,
            Ok(None) => {}
            Ok(Some(Piece::Armor(_))) => return Err("invalid native ground cache context".into()),
            Err(e) => {
                note(&mut output.errors, format!("{} ground: {e}", s.item));
                continue;
            }
        }
        let id = java_protocol::target::ITEM_TYPES
            .iter()
            .position(|name| *name == s.item)
            .ok_or("unknown native ground item")? as i32;
        let damage = equipment::component(&s, "minecraft:damage")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0) as i32;
        output.entries.insert(
            ground.id,
            Entry {
                signature: Signature {
                    revision: ground.revision,
                    slim: false,
                    left: false,
                },
                ground: true,
                seed: i64::from(id.wrapping_add(damage)),
                draws,
            },
        );
    }
    cache.retain(|key, _| used.contains(key));
    let (artwork, locations) = base.clone().with_equipment_rasters(&rasters);
    output.artwork = artwork;
    for ((id, index), location) in indices.into_iter().zip(locations) {
        output.entries.get_mut(&id).unwrap().draws[index].artwork =
            location.ok_or("native equipment texture budget exhausted")?;
    }

    Ok(output)
}
fn placeholder() -> render_model::ActorArtworkLocation {
    render_model::ActorArtworkLocation {
        page: 0,
        layer: 0,
        pose_mode: ::assets::ActorPoseMode::CompiledLiteral,
        multitexture: None,
    }
}

fn note(errors: &mut Vec<String>, message: String) {
    if errors.len() < 32 {
        errors.push(message)
    }
}

fn charge(total: &mut usize, more: usize, budget: usize) -> Result<(), String> {
    *total = total
        .checked_add(more)
        .filter(|n| *n <= budget)
        .ok_or("native equipment snapshot budget exhausted")?;
    Ok(())
}
