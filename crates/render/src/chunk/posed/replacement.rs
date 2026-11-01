//! Temporary model visibility edits share the static arena; originals remain immutable.
use super::*;
use std::collections::BTreeSet;

pub(super) fn prepare(
    scene: Res<PosedBlocks>,
    textures: Res<ChunkGpuTextureAssets>,
    arena: Res<ChunkGpuArena>,
    queue: Res<RenderQueue>,
    mut previous: Local<BTreeSet<[i32; 3]>>,
) {
    let current: BTreeSet<_> = if scene.identity.is_some()
        && textures
            .prepared
            .as_ref()
            .is_some_and(|t| Some(t.identity) == scene.identity)
    {
        scene.replaced.iter().copied().collect()
    } else {
        BTreeSet::new()
    };
    if current.is_empty() && previous.is_empty() {
        return;
    }
    let touched: BTreeSet<_> = current.union(&previous).copied().collect();
    let sections: BTreeSet<_> = touched
        .iter()
        .map(|p| p.map(|n| n.div_euclid(16)))
        .collect();
    for allocation in arena.allocations.values() {
        let key = allocation.gpu.key;
        let section = [key.x, key.y, key.z];
        if !sections.contains(&section) {
            continue;
        }
        let Some(range) = &allocation.model_range else {
            continue;
        };
        for (i, model) in allocation.models.iter().enumerate() {
            let words = model.words();
            if let Some(mask) = replacement_mask(section, words, &touched, &current) {
                let offset = (u64::from(range.start) + i as u64 * 4 + 3) * 4;
                queue.profiled_write_buffer(
                    &arena.geometry_stream_buffer,
                    offset,
                    &mask.to_ne_bytes(),
                );
            }
        }
    }
    // Reapply while active: uploads and arena migrations can replace the buffer in any frame.
    *previous = current;
}

fn replacement_mask(
    section: [i32; 3],
    words: [u32; 4],
    touched: &BTreeSet<[i32; 3]>,
    current: &BTreeSet<[i32; 3]>,
) -> Option<u32> {
    let local = [words[0] & 15, (words[0] >> 4) & 15, (words[0] >> 8) & 15];
    let p = std::array::from_fn(|i| section[i] * 16 + local[i] as i32);
    touched
        .contains(&p)
        .then(|| if current.contains(&p) { 0 } else { words[3] })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn replacement_restores_authored_partial_masks_and_preserves_unrelated_models() {
        let touched = BTreeSet::from([[-17, 48, 320]]);
        let words = [15, 7, 1024, 0x15];
        assert_eq!(
            replacement_mask([-2, 3, 20], words, &touched, &touched),
            Some(0)
        );
        assert_eq!(
            replacement_mask([-2, 3, 20], words, &touched, &BTreeSet::new()),
            Some(0x15)
        );
        assert_eq!(
            replacement_mask([-1, 3, 20], words, &touched, &touched),
            None
        );
        // The replacement is positional, independent of the old template or relocated light address.
        assert_eq!(
            replacement_mask([-2, 3, 20], [15, 99, 8000, 0xff], &touched, &touched),
            Some(0)
        );
    }
}
