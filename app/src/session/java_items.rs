//! Item head skins reuse the bounded public-profile/skin services and native defaults.
use assets::registry::java::items::{HeadTexture, HeadTextures, profile_key};
use client_shell::player_skin::java::{cache, profiles};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
    time::Duration,
};
/// The receiver-backed caches have one mutable owner. Mutex makes that owner a shared ECS resource;
/// exclusive system access uses get_mut, so profile polling never contends on a frame lock.
#[derive(bevy::prelude::Resource, Default)]
pub(crate) struct Heads(std::sync::Mutex<HeadState>);
impl Heads {
    pub fn clear(&mut self) {
        self.0.get_mut().unwrap().clear();
    }
    pub fn update(
        &mut self,
        scope: (u64, u64),
        world: &client_world::java::WorldState,
        now: Duration,
    ) -> Arc<HeadTextures> {
        self.0.get_mut().unwrap().update(scope, world, now)
    }
}
#[derive(Default)]
struct HeadState {
    scope: Option<(u64, u64)>,
    receipt: Option<(u64, u64)>,
    descriptors: BTreeMap<String, super::java_heads::Head>,
    profiles: profiles::Cache,
    skins: cache::Cache,
    identity: Vec<(String, String)>,
    failures: BTreeSet<(String, String)>,
    output: Arc<HeadTextures>,
}
impl HeadState {
    pub fn clear(&mut self) {
        if self.scope.take().is_none() {
            return;
        }
        self.receipt = None;
        self.descriptors.clear();
        self.identity.clear();
        self.failures.clear();
        self.profiles.scope(None);
        self.skins.scope(None);
        self.output = Arc::new(HeadTextures::default());
    }
    pub fn update(
        &mut self,
        scope: (u64, u64),
        world: &client_world::java::WorldState,
        now: Duration,
    ) -> Arc<HeadTextures> {
        if self.scope != Some(scope) {
            self.clear();
            self.scope = Some(scope);
        }
        let receipt = (world.inventory.revision, world.equipment.revision());
        if self.receipt != Some(receipt) {
            self.descriptors.clear();
            self.failures.clear();
            for stack in stacks(world) {
                if let Some(value) = profile(stack) {
                    match super::java_heads::Head::item(value) {
                        Ok(head) => {
                            self.descriptors.insert(profile_key(value), head);
                        }
                        Err(error) => {
                            bevy::log::warn!(target:"java_session",%error,"Invalid native item head profile")
                        }
                    }
                }
            }
            self.receipt = Some(receipt);
        }
        let queries = self
            .descriptors
            .values()
            .filter_map(|h| h.query.clone())
            .collect();
        self.profiles.update(scope, now, &queries);
        for head in self.descriptors.values_mut() {
            head.resolve(&self.profiles);
        }
        let desired = self
            .descriptors
            .values()
            .filter_map(|h| h.texture.clone())
            .collect::<BTreeSet<_>>();
        self.skins.update(scope, now, &desired);
        let images = self.skins.images(&desired);
        let mut identity = Vec::new();
        let mut textures = BTreeMap::new();
        for (key, head) in &self.descriptors {
            let failure = head
                .query
                .as_ref()
                .and_then(|q| self.profiles.failure(q))
                .or_else(|| head.texture.as_ref().and_then(|t| self.skins.failure(t)));
            if let Some(error) = failure
                && self.failures.insert((key.clone(), error.into()))
            {
                bevy::log::warn!(target:"java_session",%error,"Native item head uses UUID default while public resource is unavailable");
            }
            if head.patch.is_none()
                && let Some((name, image)) = head
                    .texture
                    .as_ref()
                    .and_then(|t| images.get_key_value(&t.head_atlas_name()))
            {
                identity.push((key.clone(), name.clone()));
                textures.insert(key.clone(), HeadTexture::Image(image.clone()));
            } else {
                let path = head
                    .patch
                    .as_ref()
                    .map(ToString::to_string)
                    .unwrap_or_else(|| head.default_texture().into());
                identity.push((key.clone(), path.clone()));
                textures.insert(key.clone(), HeadTexture::Pack(path));
            }
        }
        if self.identity != identity {
            self.identity = identity;
            self.output = Arc::new(HeadTextures {
                revision: self.output.revision.wrapping_add(1),
                textures,
            });
        }
        self.output.clone()
    }
}

/// Every publisher observes the same union, so HUD and world frames cannot evict each other.
fn stacks(world: &client_world::java::WorldState) -> impl Iterator<Item = &inventory::java::Stack> {
    let inventory = world
        .inventory
        .player
        .iter()
        .chain(world.inventory.opened.iter())
        .flat_map(|menu| {
            (0..menu.len())
                .filter_map(move |i| world.inventory.slot(menu.container, i))
                .chain(menu.carried.iter())
                .flatten()
        });
    let remote = world.entities.iter().flat_map(|entity| {
        world
            .equipment
            .get(entity.identity)
            .into_iter()
            .flat_map(|gear| gear.slots.iter().flatten())
            .chain(
                world
                    .equipment
                    .ground(entity.identity)
                    .and_then(|ground| ground.stack.as_ref()),
            )
    });
    inventory.chain(remote).chain(
        world
            .equipment
            .pickups()
            .filter_map(|pickup| pickup.ground.stack.as_ref()),
    )
}
fn profile(stack: &inventory::java::Stack) -> Option<&serde_json::Value> {
    (!stack.removed.contains("minecraft:profile"))
        .then(|| stack.added.get("minecraft:profile"))
        .flatten()
        .filter(|value| !value.is_null())
}

#[cfg(test)]
mod tests {
    use super::*;
    use java_protocol::session::{Envelope, Event, entities as wire};

    fn head(name: &str) -> inventory::java::Stack {
        inventory::java::Stack {
            item: "minecraft:player_head".into(),
            count: 1,
            added: [(
                "minecraft:profile".into(),
                serde_json::json!({"name":name,"id":[0,0,0,1],"properties":[]}),
            )]
            .into(),
            removed: Default::default(),
        }
    }
    #[test]
    fn publishers_share_inventory_and_remote_profiles_and_retire_old_scopes() {
        let mut world = client_world::java::WorldState::default();
        world.begin(7);
        world.inventory.begin(7, 0);
        world.entities.begin(7, 0);
        use java_protocol::session::facts;
        world.bootstrap = Some(Box::new(facts::Bootstrap {
            generation: 7,
            profile: java_protocol::auth::Profile {
                id: Default::default(),
                name: "Fixture".into(),
            },
            server_session: Default::default(),
            configuration: Default::default(),
            world: facts::World {
                entity_id: 42,
                hardcore: false,
                worlds: vec!["minecraft:overworld".into()],
                max_players: 2,
                view_distance: 2,
                simulation_distance: 2,
                reduced_debug: false,
                respawn_screen: true,
                limited_crafting: false,
                online_mode: false,
                secure_chat: false,
                spawn: facts::Spawn {
                    dimension_type: 0,
                    world: "minecraft:overworld".into(),
                    hashed_seed: 0,
                    game_mode: 0,
                    previous_game_mode: -1,
                    debug: false,
                    flat: true,
                    last_death: None,
                    portal_cooldown: 0,
                    sea_level: 63,
                },
            },
        }));
        world
            .inventory
            .admit(
                7,
                0,
                inventory::java::Update::PlayerSlot {
                    slot: 0,
                    stack: Some(head("Inventory")),
                },
            )
            .unwrap();
        world
            .entities
            .spawn(
                (7, 0),
                world::java::entities::Spawn {
                    transport_lifetime: 1,
                    network_id: 99,
                    uuid: [1; 16],
                    kind: "minecraft:player".into(),
                    motion: Default::default(),
                    head_yaw: 0.,
                    object_data: 0,
                    visual: Default::default(),
                    interpolation_interval: None,
                },
            )
            .unwrap();
        let remote = head("Remote");
        let packet = wire::Packet::Equipment(wire::equipment::Packet {
            id: 99,
            lifetime: 1,
            slots: [(
                5,
                Some(java_protocol::session::inventory::Stack {
                    item: java_protocol::target::ITEM_TYPES
                        .iter()
                        .position(|name| *name == remote.item)
                        .unwrap() as u32,
                    count: remote.count,
                    added: [("minecraft:profile", profile(&remote).unwrap().clone())].into(),
                    removed: Default::default(),
                }),
            )]
            .into(),
        });
        assert!(world.admit(Envelope {
            generation: 7,
            event: Event::Entities {
                dimension_epoch: 0,
                packet
            }
        }));
        let mut heads = Heads::default();
        let hud = heads.update((7, 0), &world, Duration::ZERO);
        assert_eq!(hud.textures.len(), 2);
        let actors = heads.update((7, 0), &world, Duration::ZERO);
        assert!(Arc::ptr_eq(&hud, &actors));
        assert!(
            actors
                .textures
                .contains_key(&profile_key(profile(&remote).unwrap()))
        );
        world.begin(8);
        let next = heads.update((8, 0), &world, Duration::ZERO);
        assert!(next.textures.is_empty());
        assert!(heads.0.get_mut().unwrap().descriptors.is_empty());
    }
}
