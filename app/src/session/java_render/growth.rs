//! Cancellable CPU preparation followed by checked GPU publication of new native block graphics.
use super::assets;
use ::assets::registry::java::graphics::Extension;
use render_scene::{ChunkTextureAssets, ChunkTextureBindingIdentity, ChunkTextureReload};
use std::sync::{Arc, atomic::AtomicBool};
use world::java::{Dimension, Terrain};

type ResultMessage = Result<(Dimension, Extension), String>;
#[derive(Default)]
pub(super) struct Growth {
    pending: Option<crossbeam_channel::Receiver<ResultMessage>>,
    staged: Option<(Extension, ChunkTextureAssets)>,
    publishing: Option<ChunkTextureBindingIdentity>,
    pub completed: u64,
}
impl Growth {
    pub fn busy(&self) -> bool {
        self.pending.is_some() || self.staged.is_some() || self.publishing.is_some()
    }
    /// Keeps old geometry resident until its compatible GPU tables are ready. Dimension reset
    /// drops this mailbox and cancels its source; late CPU/GPU results cannot reach a new world.
    pub fn tick(
        &mut self,
        stream: &mut chunk_pipeline::java::Stream,
        terrain: &Terrain,
        textures: &mut ChunkTextureAssets,
        gpu: &ChunkTextureReload,
        cancel: &Arc<AtomicBool>,
    ) -> Result<(), String> {
        if let Some(identity) = self.publishing {
            if let Some(requested) = gpu.requested() {
                if requested.binding_identity() != identity {
                    return Err("Java graphics publication was superseded".into());
                }
                return Ok(());
            }
            self.publishing = None;
        }
        if let Some(receiver) = &self.pending {
            match receiver.try_recv() {
                Ok(result) => {
                    self.pending = None;
                    let (dimension, extension) = result?;
                    if terrain.dimension() != Some(&dimension)
                        || !extension.extends(stream.graphics())
                    {
                        client_runtime::session::release_off_frame(extension);
                        return Ok(());
                    }
                    let candidate = ChunkTextureAssets::with_revision(
                        Arc::clone(&extension.graphics().runtime),
                        textures.revision.saturating_add(1),
                    );
                    if !gpu.request_extension(
                        gpu.geometry_epoch(),
                        textures,
                        candidate.clone(),
                        extension.resources(),
                    ) {
                        client_runtime::session::release_off_frame(extension);
                        return Err(
                            "Java graphics extension no longer matches the active atlas".into()
                        );
                    }
                    self.staged = Some((extension, candidate));
                }
                Err(crossbeam_channel::TryRecvError::Empty) => return Ok(()),
                Err(crossbeam_channel::TryRecvError::Disconnected) => {
                    self.pending = None;
                    return Err("Java graphics worker stopped".into());
                }
            }
        }
        if let Some((_, candidate)) = &self.staged {
            if !gpu_ready(gpu, candidate.binding_identity())? {
                return Ok(());
            }
            let (extension, candidate) = self.staged.take().expect("checked staged extension");
            if !stream.install_graphics(terrain, &extension) {
                gpu.cancel_except(textures.binding_identity());
                client_runtime::session::release_off_frame((extension, candidate));
                return Ok(());
            }
            self.publishing = Some(candidate.binding_identity());
            let old = std::mem::replace(textures, candidate);
            client_runtime::session::release_off_frame((extension, old));
            self.completed += 1;
            return Ok(());
        }
        let states = stream.missing_graphics();
        if states.is_empty() {
            return Ok(());
        }
        let dimension = stream.dimension().clone();
        let previous = Arc::clone(stream.graphics());
        let cancel = Arc::clone(cancel);
        let (sender, receiver) = crossbeam_channel::bounded(1);
        std::thread::Builder::new()
            .name("java-graphics-extension".into())
            .spawn(move || {
                let _ = sender.send(
                    assets::extend(previous, states, cancel)
                        .map(|extension| (dimension, extension)),
                );
            })
            .map_err(|e| format!("Could not start Java graphics worker: {e}"))?;
        self.pending = Some(receiver);
        Ok(())
    }
}

/// A revoked GPU request is a failure, not an indefinitely pending asset dependency.
fn gpu_ready(
    gpu: &ChunkTextureReload,
    identity: ChunkTextureBindingIdentity,
) -> Result<bool, String> {
    match gpu.status(identity) {
        Some(result) => result.map(|()| true),
        None if gpu
            .requested()
            .is_some_and(|request| request.binding_identity() == identity) =>
        {
            Ok(false)
        }
        None => Err("Java graphics preparation was cancelled or superseded".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cancelled_graphics_preparation_fails_instead_of_waiting_forever() {
        let gpu = ChunkTextureReload::default();
        let assets = ChunkTextureAssets::default();
        let identity = assets.binding_identity();
        gpu.request(assets.clone());
        assert_eq!(gpu_ready(&gpu, identity), Ok(false));
        gpu.request(ChunkTextureAssets::with_revision(
            Arc::clone(assets.assets()),
            1,
        ));
        assert!(gpu_ready(&gpu, identity).is_err());
        gpu.request(assets);
        assert!(gpu.finish(&gpu.preparation().unwrap(), true));
        assert_eq!(gpu_ready(&gpu, identity), Ok(true));
        gpu.discard_geometry();
        assert!(gpu_ready(&gpu, identity).is_err());
    }
}
