//! Offline whole-registry model admission audit using the same compiler as the application.
use super::java_runtime::{Report, TARGET, verify};
use assets::registry::java::{
    BlockRegistry,
    catalog::BlockModels,
    graphics,
    source::{Error as ResourceError, Jar},
};
use clap::Args;
use serde::Deserialize;
use std::{collections::BTreeMap, error::Error, fs, io::Write, path::PathBuf, sync::Arc};

#[derive(Debug, Args)]
pub(super) struct Options {
    /// Prepared runtime directory; defaults to .local/assets/java/<pinned-version>.
    #[arg(long)]
    assets: Option<PathBuf>,
    /// JSON output; contains asset diagnostics only, never account/server data.
    #[arg(long, default_value = ".local/logs/java-model-audit.json")]
    out: PathBuf,
}

#[derive(Deserialize)]
struct Target {
    game_version: String,
    protocol: u32,
    client_assets: Report,
    block_registry_report: Report,
    fluid_render_report: Option<Report>,
}

pub(super) fn run(options: Options) -> Result<(), Box<dyn Error>> {
    let target: Target = serde_json::from_str(TARGET)?;
    run_target(options, target)
}

fn run_target(options: Options, target: Target) -> Result<(), Box<dyn Error>> {
    let root = options
        .assets
        .unwrap_or_else(|| PathBuf::from(".local/assets/java").join(&target.game_version));
    let blocks = root.join("blocks.json");
    let client = root.join("client.jar");
    for input in [&blocks, &client] {
        super::validate_output_bundle(input, &options.out)?;
    }
    verify(
        &blocks,
        &target.block_registry_report.sha256,
        target.block_registry_report.bytes,
    )?;
    let registry = Arc::new(BlockRegistry::load(
        &fs::read(blocks)?,
        &target.block_registry_report.sha256,
    )?);
    let jar = Jar::open(
        &client,
        &target.client_assets.sha256,
        target.client_assets.bytes,
    )?;
    let fluids = target
        .fluid_render_report
        .as_ref()
        .map(|pin| -> Result<_, Box<dyn Error>> {
            let path = root.join("fluids.json");
            super::validate_output_bundle(&path, &options.out)?;
            verify(&path, &pin.sha256, pin.bytes)?;
            Ok(Arc::new(assets::registry::java::fluids::Table::load(
                &fs::read(path)?,
                &pin.sha256,
                registry.len(),
            )?))
        })
        .transpose()?;
    let report = audit(Arc::clone(&registry), Box::new(jar), fluids)?;
    let document = serde_json::json!({
        "schema": 1,
        "scope": "all_pinned_block_states",
        "game_version": target.game_version,
        "protocol": target.protocol,
        "block_report_sha256": target.block_registry_report.sha256,
        "client_sha256": target.client_assets.sha256,
        "fluid_report_sha256": target.fluid_render_report.as_ref().map(|r| &r.sha256),
        "model_admission": report,
        "limitations": "First compiler failure per state, grouped with bounded samples. Does not test rendering, gameplay, network packets, or behavior hidden behind an earlier failure in the same state."
    });
    let parent = options
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&serde_json::to_vec_pretty(&document)?)?;
    file.as_file().sync_all()?;
    file.persist(&options.out)?;
    println!(
        "Checked {} states: {} failed, {} groups, {} omitted failures. Report: {}",
        report.checked_states,
        report.failed_states,
        report.issues.len(),
        report.omitted_failures,
        options.out.display()
    );
    if report.failed_states > 0 {
        return Err("Java model support is incomplete; audit saved. Do not regenerate valid assets to fix unsupported renderers.".into());
    }
    Ok(())
}

fn audit(
    registry: Arc<BlockRegistry>,
    source: Box<dyn assets::registry::java::source::Source + Send>,
    fluids: Option<Arc<assets::registry::java::fluids::Table>>,
) -> Result<graphics::audit::Report, ResourceError> {
    let mut groups = BTreeMap::<String, Vec<u32>>::new();
    for id in 0..registry.len() as u32 {
        groups
            .entry(
                registry
                    .state(id)
                    .expect("contiguous registry")
                    .name
                    .to_string(),
            )
            .or_default()
            .push(id);
    }
    let mut models = BlockModels::new(registry, source);
    models.fluids = fluids;
    let mut report = graphics::audit::Report::default();
    // Release each block's GPU-shaped buffers before the next block; this is not a world atlas.
    for (name, states) in groups {
        let count = states.len() as u64;
        match graphics::compile(
            &mut models,
            states.iter().copied(),
            assets::MAX_CARRIER_TEXTURE_LAYERS as u32,
        ) {
            Ok(_) => report.checked_states += count,
            Err(ResourceError::ModelBatch(batch)) => report.merge(batch),
            Err(error) => {
                if models.cancelled() {
                    return Err(error);
                }
                report.checked_states += count;
                for state in states {
                    report.record(state, &name, &error.to_string());
                }
            }
        }
    }
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    struct Empty;
    impl assets::registry::java::source::Source for Empty {
        fn read(&mut self, _: &str) -> Result<Vec<u8>, ResourceError> {
            Err(ResourceError::Zip(zip::result::ZipError::FileNotFound))
        }
    }
    #[test]
    fn whole_registry_audit_reaches_blocks_after_the_first_failure() {
        let bytes =
            br#"{"minecraft:first":{"states":[{"id":0,"default":true},{"id":1}]},"minecraft:last":{"states":[{"id":2,"default":true}]}}"#;
        let registry = BlockRegistry::load(bytes, &format!("{:x}", Sha256::digest(bytes))).unwrap();
        let report = audit(Arc::new(registry), Box::new(Empty), None).unwrap();
        assert_eq!(report.checked_states, 3);
        assert_eq!(report.failed_states, 3);
        assert_eq!(report.issues.len(), 2);
        assert_eq!(report.issues[0].affected_states, 2);
        assert_eq!(report.issues[1].block, "minecraft:last");
    }

    #[test]
    fn failing_audit_writes_a_report_and_never_overwrites_source_archives() {
        let root = tempfile::tempdir().unwrap();
        let bytes = br#"{"minecraft:lava":{"states":[{"id":0,"default":true}]}}"#;
        fs::write(root.path().join("blocks.json"), bytes).unwrap();
        let client = root.path().join("client.jar");
        zip::ZipWriter::new(fs::File::create(&client).unwrap())
            .finish()
            .unwrap();
        let archive = fs::read(&client).unwrap();
        let pin = || Target {
            fluid_render_report: None,
            game_version: "fixture".into(),
            protocol: 0,
            client_assets: Report {
                sha256: format!("{:x}", Sha256::digest(&archive)),
                bytes: archive.len() as u64,
            },
            block_registry_report: Report {
                sha256: format!("{:x}", Sha256::digest(bytes)),
                bytes: bytes.len() as u64,
            },
        };
        let out = root.path().join("audit.json");
        let error = run_target(
            Options {
                assets: Some(root.path().into()),
                out: out.clone(),
            },
            pin(),
        )
        .unwrap_err();
        assert!(error.to_string().contains("audit saved"));
        let document: serde_json::Value = serde_json::from_slice(&fs::read(out).unwrap()).unwrap();
        assert_eq!(document["model_admission"]["failed_states"], 1);
        assert_eq!(
            document["model_admission"]["issues"][0]["block"],
            "minecraft:lava"
        );
        assert!(
            run_target(
                Options {
                    assets: Some(root.path().into()),
                    out: client.clone()
                },
                pin()
            )
            .is_err()
        );
        assert_eq!(fs::read(client).unwrap(), archive);
    }
}
