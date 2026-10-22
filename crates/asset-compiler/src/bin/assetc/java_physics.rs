//! Offline native Java collision data preparation; never used by the running client.
use clap::Args;
use serde::Deserialize;
use std::{
    error::Error,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
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
    java_runtime_major: u32,
    server_sha256: String,
    server_bytes: u64,
    block_registry_report: Registry,
    physics_report: Report,
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
    verify(&options.server, &target.server_sha256, target.server_bytes)?;
    // Preserve caller-relative executable paths when the exporter runs in its isolated directory.
    let java = if options.java.components().count() > 1 {
        options.java.canonicalize()?
    } else {
        options.java.clone()
    };
    let work = tempfile::tempdir()?;
    let mut archive = zip::ZipArchive::new(fs::File::open(&options.server)?)?;
    let mut jars = Vec::new();
    for folder in ["versions", "libraries"] {
        let mut list = String::new();
        archive
            .by_name(&format!("META-INF/{folder}.list"))?
            .read_to_string(&mut list)?;
        if folder == "versions" && list.lines().count() != 1 {
            return Err("Java server must contain exactly one pinned version".into());
        }
        for line in list.lines() {
            let fields = line.split('\t').collect::<Vec<_>>();
            let [hash, name, path] = fields.as_slice() else {
                return Err("invalid Java bundled library inventory".into());
            };
            if folder == "versions" && *name != target.game_version {
                return Err("Java bundled version does not match target".into());
            }
            // ZIP paths are lookup keys only. Files are written under generated flat names.
            let jar = work.path().join(format!("library-{}.jar", jars.len()));
            let mut entry = archive.by_name(&format!("META-INF/{folder}/{path}"))?;
            let length = entry.size();
            std::io::copy(&mut entry, &mut fs::File::create(&jar)?)?;
            verify(&jar, hash, length)?;
            jars.push(jar);
        }
    }
    let source = work.path().join("ExportPhysics.java");
    fs::write(&source, EXPORTER)?;
    let classpath = std::env::join_paths(&jars)?;
    let release = target.java_runtime_major.to_string();
    let mut compiler = if let Some(jar) = options.compiler_jar {
        let mut command = Command::new(&java);
        command
            .args(["-Xmx512m", "-jar"])
            .arg(jar)
            .args(["-source", &release, "-target", &release]);
        command
    } else {
        let mut command = Command::new(&options.javac);
        command.args(["--release", &release]);
        command
    };
    let result = compiler
        .arg("-cp")
        .arg(&classpath)
        .arg("-d")
        .arg(work.path())
        .arg(&source)
        .status()?;
    if !result.success() {
        return Err(format!("Java physics exporter compilation failed: {result}").into());
    }
    let generated = work.path().join("physics.bin");
    let classpath = std::env::join_paths(
        std::iter::once(work.path()).chain(jars.iter().map(PathBuf::as_path)),
    )?;
    let result = Command::new(&java)
        .arg("-Xmx512m")
        .arg("-cp")
        .arg(classpath)
        .arg("ExportPhysics")
        .arg(&generated)
        .current_dir(work.path())
        .status()?;
    if !result.success() {
        return Err(format!("Java physics exporter failed: {result}").into());
    }
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

fn verify(path: &Path, sha256: &str, bytes: u64) -> Result<(), Box<dyn Error>> {
    if fs::metadata(path)?.len() != bytes || assets::vanilla_pack::sha256_file(path)? != sha256 {
        return Err(format!(
            "Java physics source hash/length mismatch: {}",
            path.display()
        )
        .into());
    }
    Ok(())
}

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
