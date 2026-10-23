//! Worker-side proof that resident draw addresses survive a resource-table extension.
use super::RuntimeAssets;
use std::sync::Arc;

#[derive(Clone)]
pub struct DrawResourceExtension {
    previous: Arc<RuntimeAssets>,
    next: Arc<RuntimeAssets>,
}

impl DrawResourceExtension {
    /// Checks all existing geometry/material addresses off-frame. Pixel content may change,
    /// but source dimensions and grids (used by UV geometry) must retain their meaning.
    pub fn checked(previous: Arc<RuntimeAssets>, next: Arc<RuntimeAssets>) -> Option<Self> {
        let a = &previous;
        let b = &next;
        if !b.visuals.starts_with(&a.visuals)
            || a.hashed != b.hashed
            || !b.light_properties.starts_with(&a.light_properties)
            || !b.materials.starts_with(&a.materials)
            || !b.model_templates.starts_with(&a.model_templates)
            || !b.model_random_offsets.starts_with(&a.model_random_offsets)
            || !b.model_quads.starts_with(&a.model_quads)
            || !b.animations.starts_with(&a.animations)
            || !b.animation_frames.starts_with(&a.animation_frames)
            || a.biomes != b.biomes
        {
            return None;
        }
        for (page, old) in a.texture_pages.iter().enumerate() {
            let new = b.texture_pages.get(page)?;
            if new.texture.layers < old.texture.layers {
                return None;
            }
            for layer in 0..old.texture.layers {
                let reference = crate::TextureRef::new(page as u32, layer).ok()?;
                if a.texture_source_size(reference) != b.texture_source_size(reference)
                    || a.texture_source_grid(reference) != b.texture_source_grid(reference)
                {
                    return None;
                }
            }
        }
        Some(Self { previous, next })
    }
    /// Identity check is constant-time; callers need not repeat the worker's table comparisons.
    pub fn matches(&self, previous: &Arc<RuntimeAssets>, next: &Arc<RuntimeAssets>) -> bool {
        Arc::ptr_eq(&self.previous, previous) && Arc::ptr_eq(&self.next, next)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changed_resident_material_or_missing_texture_cannot_claim_prefix_compatibility() {
        let old = Arc::new(RuntimeAssets::diagnostic());
        let mut changed = RuntimeAssets::diagnostic();
        changed.materials[0].flags ^= crate::MATERIAL_FLAG_ALPHA_CUTOUT;
        assert!(DrawResourceExtension::checked(Arc::clone(&old), Arc::new(changed)).is_none());
        let mut missing = RuntimeAssets::diagnostic();
        missing.texture_pages = Box::new([]);
        assert!(DrawResourceExtension::checked(Arc::clone(&old), Arc::new(missing)).is_none());
        let next = Arc::new(RuntimeAssets::diagnostic());
        let proof = DrawResourceExtension::checked(Arc::clone(&old), Arc::clone(&next)).unwrap();
        assert!(proof.matches(&old, &next));
        assert!(!proof.matches(&old, &Arc::new(RuntimeAssets::diagnostic())));
    }
}
