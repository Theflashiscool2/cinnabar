//! Generic entity texture admission, independent of any edition's asset catalog format.
use super::*;
impl ActorArtworkPages {
    /// Builds immutable artwork off-frame from prepared pages and explicit rig bindings.
    /// Page indices in `bindings` are zero-based; GPU page zero remains reserved for player skins.
    pub fn from_entity_pages(
        pages: Vec<ActorTexturePage>,
        bindings: &[(EntityRigId, usize, u32)],
    ) -> Result<Self, &'static str> {
        if pages.is_empty() || pages.len() >= MAX_ACTOR_TEXTURE_PAGES {
            return Err("entity artwork page count");
        }
        let mut total = player_page_bytes();
        let mut hash = Sha256::new();
        let mut locations = BTreeSet::new();
        for (index, page) in pages.iter().enumerate() {
            let bytes = usize::from(page.width)
                .checked_mul(usize::from(page.height))
                .and_then(|n| n.checked_mul(page.layers as usize))
                .and_then(|n| n.checked_mul(4))
                .ok_or("entity artwork byte count overflow")?;
            if bytes == 0
                || bytes != page.rgba8.len()
                || page.layers as usize > MAX_ACTOR_PAGE_LAYERS
                || page.color_mask
                || page.multitexture
            {
                return Err("invalid generic entity texture page");
            }
            total = total
                .checked_add(bytes)
                .ok_or("entity artwork byte budget overflow")?;
            if total > MAX_ACTOR_GPU_PIXEL_BYTES {
                return Err("entity artwork exceeds shared GPU pixel budget");
            }
            hash.update(page.width.to_le_bytes());
            hash.update(page.height.to_le_bytes());
            hash.update(page.layers.to_le_bytes());
            hash.update(&page.rgba8);
            for layer in 0..page.layers {
                locations.insert(((index + 1) as ActorArtworkPageId, layer));
            }
        }
        let mut routes = BTreeMap::new();
        for &(rig, page, layer) in bindings {
            let page_id = ActorArtworkPageId::try_from(
                page.checked_add(1).ok_or("entity artwork page overflow")?,
            )
            .map_err(|_| "entity artwork page overflow")?;
            if !locations.contains(&(page_id, layer))
                || crate::is_equipment_rig_id(rig)
                || crate::is_pack_rig_id(rig)
                || crate::is_layer_geometry_rig_id(rig)
                || rig == crate::DIAGNOSTIC_RIG_ID
            {
                return Err("entity artwork binding outside its catalog");
            }
            let route = ActorArtworkLocation {
                page: page_id,
                layer,
                pose_mode: assets::ActorPoseMode::CompiledLiteral,
                multitexture: None,
            };
            if routes.insert(rig, route).is_some() {
                return Err("duplicate entity artwork rig binding");
            }
            hash.update(rig.0.to_le_bytes());
            hash.update(page_id.to_le_bytes());
            hash.update(layer.to_le_bytes());
        }
        let identity = hash.finalize().into();
        Ok(Self {
            identity,
            entity_identity: identity,
            pages: pages.into(),
            routes: Arc::new(routes),
            entity_locations: Arc::new(locations),
            ..Default::default()
        })
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn page() -> ActorTexturePage {
        ActorTexturePage {
            width: 1,
            height: 1,
            layers: 1,
            rgba8: Arc::from([255, 0, 0, 255]),
            color_mask: false,
            multitexture: false,
        }
    }
    #[test]
    fn native_pages_have_real_routes_and_cannot_bind_outside_their_own_pixels() {
        let rig = EntityRigId(17);
        let artwork = ActorArtworkPages::from_entity_pages(vec![page()], &[(rig, 0, 0)]).unwrap();
        let route = artwork.route(rig).unwrap();
        assert_eq!((route.page, route.layer), (1, 0));
        assert!(artwork.valid(rig, route));
        assert!(!artwork.valid(EntityRigId(18), route));
        assert!(ActorArtworkPages::from_entity_pages(vec![page()], &[(rig, 0, 1)]).is_err());
        assert!(ActorArtworkPages::from_entity_pages(vec![page()], &[(rig, 1, 0)]).is_err());
        assert!(
            ActorArtworkPages::from_entity_pages(vec![page()], &[(rig, 0, 0), (rig, 0, 0)])
                .is_err()
        );
        assert!(
            ActorArtworkPages::from_entity_pages(
                vec![page()],
                &[(crate::equipment_rig_id(0), 0, 0)]
            )
            .is_err()
        );
        let mut malformed = page();
        malformed.rgba8 = Arc::from([0]);
        assert!(ActorArtworkPages::from_entity_pages(vec![malformed], &[(rig, 0, 0)]).is_err());
    }
}
