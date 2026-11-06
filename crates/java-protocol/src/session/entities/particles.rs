//! Native ParticleOptions framing, independent of optional particle rendering.
use crate::{ProtocolError as Error, target, wire::Reader};
pub(super) fn read(r: &mut Reader<'_>) -> Result<(), Error> {
    let id = usize::try_from(r.var()?).map_err(|_| Error::Invalid("negative native particle"))?;
    let spec = target::PARTICLES
        .get(id)
        .ok_or(Error::Invalid("unknown native particle"))?;
    let kind = spec
        .options
        .strip_prefix("net.minecraft.core.particles.")
        .unwrap_or(spec.options);
    match kind {
        "simple" => {}
        "BlockParticleOption" => {
            let state = r.var()?;
            if state < 0 || state as usize >= target::BLOCK_STATE_COUNT {
                return Err(Error::Invalid("native particle block state"));
            }
        }
        "ColorParticleOption" | "GeyserParticleOptions" => {
            r.i32()?;
        }
        "DustParticleOptions" | "SpellParticleOption" | "GeyserBaseParticleOptions" => {
            r.i32()?;
            r.f32()?;
        }
        "DustColorTransitionOptions" => {
            r.i32()?;
            r.i32()?;
            r.f32()?;
        }
        "PowerParticleOption" | "SculkChargeParticleOptions" => {
            r.f32()?;
        }
        "ShriekParticleOption" => {
            r.var()?;
        }
        "ItemParticleOption" => super::super::inventory::particle_template(r)?,
        "TrailParticleOption" => {
            for _ in 0..3 {
                r.f64()?;
            }
            r.i32()?;
            r.var()?;
        }
        "VibrationParticleOption" => {
            let id = usize::try_from(r.var()?)
                .map_err(|_| Error::Invalid("negative native position source"))?;
            match target::POSITION_SOURCE_TYPES.get(id).copied() {
                Some("minecraft:block") => {
                    r.i64()?;
                }
                Some("minecraft:entity") => {
                    r.var()?;
                    r.f32()?;
                }
                _ => return Err(Error::Invalid("unknown native position source")),
            }
            r.var()?;
        }
        _ => return Err(Error::Invalid("unimplemented native particle options")),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn every_pinned_particle_matches_independent_native_codec_vectors() {
        // Synthetic fields were decoded and re-encoded byte-equal by pinned ParticleTypes.STREAM_CODEC.
        let rows: serde_json::Value =
            serde_json::from_str(include_str!("../../../data/particle-options-fixture.json"))
                .unwrap();
        assert_eq!(rows.as_array().unwrap().len(), target::PARTICLES.len());
        for (id, row) in rows.as_array().unwrap().iter().enumerate() {
            assert_eq!(row["name"], target::PARTICLES[id].name);
            assert_eq!(row["options"], target::PARTICLES[id].options);
            let hex = row["hex"].as_str().unwrap();
            let b = (0..hex.len())
                .step_by(2)
                .map(|i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap())
                .collect::<Vec<_>>();
            let mut r = Reader(&b);
            read(&mut r).unwrap();
            r.finish().unwrap();
            for end in 0..b.len() {
                assert!(
                    read(&mut Reader(&b[..end])).is_err(),
                    "{} truncated at {end}",
                    row["name"]
                );
            }
        }
    }
    #[test]
    fn entity_position_source_and_unknown_particle_ids_do_not_desynchronize() {
        let id = target::PARTICLES
            .iter()
            .position(|p| p.options.ends_with("VibrationParticleOption"))
            .unwrap();
        let source = target::POSITION_SOURCE_TYPES
            .iter()
            .position(|s| *s == "minecraft:entity")
            .unwrap();
        let mut b = Vec::new();
        crate::codec::write_varint(id as i32, &mut b);
        crate::codec::write_varint(source as i32, &mut b);
        b.push(10);
        b.extend(1.25f32.to_be_bytes());
        b.push(12);
        let mut r = Reader(&b);
        read(&mut r).unwrap();
        r.finish().unwrap();
        let mut bad = Vec::new();
        crate::codec::write_varint(target::PARTICLES.len() as i32, &mut bad);
        assert!(read(&mut Reader(&bad)).is_err());
    }
}
