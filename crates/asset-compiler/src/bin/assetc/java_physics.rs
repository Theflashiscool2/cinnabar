//! Offline native Java collision data preparation; never used by the running client.
use clap::Args;
use serde::Deserialize;
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

const TARGET: &str = include_str!("../../../../java-protocol/data/target.json");
const EXPORTER: &str = include_str!("java/ExportPhysics.java");

#[derive(Debug, Args)]
pub(super) struct Options {
    /// Official server archive for the pinned Java release; verified before any code executes.
    #[arg(long)]
    pub(super) server: PathBuf,
    /// Runtime physics report destination, normally alongside Java client.jar and blocks.json.
    #[arg(long)]
    pub(super) out: PathBuf,
    #[arg(long, default_value = "java")]
    pub(super) java: PathBuf,
    #[arg(long, default_value = "javac", conflicts_with = "compiler_jar")]
    pub(super) javac: PathBuf,
    /// Optional local Eclipse ECJ compiler, for a JRE without javac; never shipped in the client.
    #[arg(long)]
    pub(super) compiler_jar: Option<PathBuf>,
    /// Confirms acceptance of the Minecraft EULA for processing the supplied server assets.
    #[arg(long)]
    accept_eula: bool,
}

#[derive(Deserialize)]
struct Target {
    game_version: String,
    block_registry_report: Registry,
    physics_report: Report,
    player_attributes_report: Report,
}
#[derive(Deserialize)]
struct Registry {
    states: usize,
}
#[derive(Deserialize)]
struct Report {
    sha256: String,
    bytes: u64,
}

/// Verifies the archive and every bundled library, then runs only the original data exporter.
pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java physics preparation requires explicit --accept-eula".into());
    }
    let target: Target = serde_json::from_str(TARGET)?;
    let runtime = super::java_runtime::Runtime::new(&options.server, &options.java)?;
    let generated = runtime.path().join("physics.bin");
    let player_attributes = runtime.path().join("player-attributes.json");
    runtime.export(
        "ExportPhysics",
        EXPORTER,
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&generated, &player_attributes],
    )?;
    verify(
        &player_attributes,
        &target.player_attributes_report.sha256,
        target.player_attributes_report.bytes,
    )?;
    verify(
        &generated,
        &target.physics_report.sha256,
        target.physics_report.bytes,
    )?;
    assets::registry::java::physics::Table::load(
        &fs::read(&generated)?,
        &target.physics_report.sha256,
        target.block_registry_report.states,
    )?;
    let parent = options
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&fs::read(&generated)?)?;
    staged.as_file().sync_all()?;
    staged.persist(&options.out)?;
    println!(
        "Verified Java {} physics report: {}",
        target.game_version,
        options.out.display()
    );
    Ok(())
}

use super::java_runtime::verify;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn java_physics_cli_preserves_input_output_alias_checks() {
        use clap::Parser;
        for source in ["server.jar", "compiler.jar"] {
            let parsed = crate::cli::Cli::try_parse_from([
                "assetc",
                "java-physics",
                "--server",
                "server.jar",
                "--compiler-jar",
                "compiler.jar",
                "--out",
                source,
                "--accept-eula",
            ])
            .unwrap();
            let error = crate::run(parsed.command).unwrap_err().to_string();
            assert!(error.contains("distinct files"), "{error}");
        }
    }
    #[test]
    fn preparation_requires_eula_and_rejects_unverified_executable_input() {
        let root = tempfile::tempdir().unwrap();
        let server = root.path().join("server.jar");
        fs::write(&server, b"untrusted").unwrap();
        let options = |accept_eula| Options {
            server: server.clone(),
            out: root.path().join("out.bin"),
            java: root.path().join("must-not-run"),
            javac: root.path().join("must-not-run"),
            compiler_jar: None,
            accept_eula,
        };
        assert!(
            generate(options(false))
                .unwrap_err()
                .to_string()
                .contains("--accept-eula")
        );
        assert!(
            generate(options(true))
                .unwrap_err()
                .to_string()
                .contains("hash/length mismatch")
        );
        assert!(!root.path().join("out.bin").exists());
    }

    #[test]
    fn authorized_pinned_server_export_matches_native_collision_facts() {
        let Some(server) = std::env::var_os("CINNABAR_TEST_JAVA_PHYSICS_SERVER") else {
            eprintln!("missing fixture: CINNABAR_TEST_JAVA_PHYSICS_SERVER");
            return;
        };
        if std::env::var("CINNABAR_TEST_JAVA_EULA_ACCEPTED").as_deref() != Ok("1") {
            eprintln!("missing fixture: explicit CINNABAR_TEST_JAVA_EULA_ACCEPTED=1");
            return;
        }
        let Some(report) = std::env::var_os("CINNABAR_TEST_JAVA_BLOCK_REPORT") else {
            eprintln!("missing fixture: CINNABAR_TEST_JAVA_BLOCK_REPORT");
            return;
        };
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("physics.bin");
        generate(Options {
            server: server.into(),
            out: out.clone(),
            accept_eula: true,
            java: std::env::var_os("CINNABAR_TEST_JAVA_RUNTIME")
                .unwrap_or_else(|| "java".into())
                .into(),
            javac: "javac".into(),
            compiler_jar: std::env::var_os("CINNABAR_TEST_JAVA_COMPILER_JAR").map(Into::into),
        })
        .unwrap();
        let target: Target = serde_json::from_str(TARGET).unwrap();
        let manifest: serde_json::Value = serde_json::from_str(TARGET).unwrap();
        let registry = assets::registry::java::BlockRegistry::load(
            &fs::read(report).unwrap(),
            manifest["block_registry_report"]["sha256"]
                .as_str()
                .unwrap(),
        )
        .unwrap();
        let table = assets::registry::java::physics::Table::load(
            &fs::read(out).unwrap(),
            &target.physics_report.sha256,
            registry.len(),
        )
        .unwrap();
        let state = |name| {
            (0..registry.len() as u32)
                .find_map(|id| {
                    let identity = registry.state(id)?;
                    (identity.is_default && &*identity.name == name)
                        .then(|| table.state(id).unwrap())
                })
                .unwrap()
        };
        use assets::registry::java::physics::{Bounds, Collision};
        let grass = state("minecraft:grass_block");
        let Collision::Static(boxes) = &grass.collision else {
            panic!("static grass")
        };
        assert_eq!(
            boxes.as_ref(),
            &[Bounds {
                min: [0.; 3],
                max: [1.; 3]
            }]
        );
        assert_eq!(grass.friction, 0.6);
        assert!(!grass.special_callbacks);
        let air = state("minecraft:air");
        assert!(air.is_air && air.fluid_empty);
        assert!(matches!(&air.collision, Collision::Static(boxes) if boxes.is_empty()));
        let stair = state("minecraft:oak_stairs");
        let Collision::Static(boxes) = &stair.collision else {
            panic!("static stair")
        };
        assert_eq!(
            boxes.as_ref(),
            &[
                Bounds {
                    min: [0.; 3],
                    max: [1., 0.5, 1.]
                },
                Bounds {
                    min: [0., 0.5, 0.],
                    max: [1., 1., 0.5]
                },
            ]
        );
        assert!(!stair.special_callbacks);
        assert!(matches!(
            state("minecraft:scaffolding").collision,
            Collision::Dynamic
        ));
        assert!(state("minecraft:slime_block").special_callbacks);
        let water = state("minecraft:water");
        assert!(!water.is_air && !water.fluid_empty);
        assert_eq!(state("minecraft:ice").friction, 0.98);
    }
}
