//! Offline extraction of native model geometry; copyrighted model data is never embedded.
use super::java_runtime::{Report, Runtime, TARGET, verify};
use clap::Args;
use serde::Deserialize;
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
};

#[derive(Debug, Args)]
pub(super) struct Options {
    #[arg(long)]
    pub server: PathBuf,
    #[arg(long)]
    pub client: PathBuf,
    /// Runtime models.json, alongside the native Java client.jar and physics.bin.
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long, default_value = "java")]
    pub java: PathBuf,
    #[arg(long, default_value = "javac", conflicts_with = "compiler_jar")]
    pub javac: PathBuf,
    #[arg(long)]
    pub compiler_jar: Option<PathBuf>,
    /// Explicit acceptance of the Minecraft EULA for processing the supplied game assets.
    #[arg(long)]
    pub(super) accept_eula: bool,
}
#[derive(Deserialize)]
struct Target {
    entity_models_report: Report,
    entity_facts_report: Report,
}

pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java entity preparation requires explicit --accept-eula".into());
    }
    let pin: Target = serde_json::from_str(TARGET)?;
    let mut runtime = Runtime::new(&options.server, &options.java)?;
    runtime.add_client(&options.client)?;
    let models = runtime.path().join("models.json");
    let facts = runtime.path().join("entity-facts.json");
    runtime.export(
        "ExportEntities",
        include_str!("java/ExportEntities.java"),
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&models, &facts],
    )?;
    verify(
        &models,
        &pin.entity_models_report.sha256,
        pin.entity_models_report.bytes,
    )?;
    verify(
        &facts,
        &pin.entity_facts_report.sha256,
        pin.entity_facts_report.bytes,
    )?;
    let bytes = fs::read(&models)?;
    let parsed = assets::entity::java::Models::load(
        &bytes,
        &pin.entity_models_report.sha256,
        pin.entity_models_report.bytes,
    )?;
    let parent = options
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    fs::create_dir_all(parent)?;
    let mut staged = tempfile::NamedTempFile::new_in(parent)?;
    staged.write_all(&bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(&options.out)?;
    println!(
        "Verified {} native Java models: {}",
        parsed.iter().count(),
        options.out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn aliases_and_unverified_inputs_never_execute_exporters() {
        use clap::Parser;
        for source in ["server.jar", "client.jar", "compiler.jar"] {
            let parsed = crate::cli::Cli::try_parse_from([
                "assetc",
                "java-entities",
                "--server",
                "server.jar",
                "--client",
                "client.jar",
                "--compiler-jar",
                "compiler.jar",
                "--out",
                source,
                "--accept-eula",
            ])
            .unwrap();
            assert!(
                crate::run(parsed.command)
                    .unwrap_err()
                    .to_string()
                    .contains("distinct files")
            );
        }
        let dir = tempfile::tempdir().unwrap();
        let server = dir.path().join("server.jar");
        fs::write(&server, b"unverified").unwrap();
        for accepted in [false, true] {
            let error = generate(Options {
                server: server.clone(),
                client: dir.path().join("client.jar"),
                out: dir.path().join("out.json"),
                java: dir.path().join("must-not-run"),
                javac: "must-not-run".into(),
                compiler_jar: None,
                accept_eula: accepted,
            })
            .unwrap_err()
            .to_string();
            assert!(
                error.contains(if accepted {
                    "hash/length mismatch"
                } else {
                    "--accept-eula"
                }),
                "{error}"
            );
            assert!(!dir.path().join("out.json").exists());
        }
    }
    #[test]
    fn authorized_pinned_models_export_reproduces_geometry_and_schemas() {
        let (Some(server), Some(client)) = (
            std::env::var_os("CINNABAR_TEST_JAVA_PHYSICS_SERVER"),
            std::env::var_os("CINNABAR_TEST_JAVA_CLIENT"),
        ) else {
            eprintln!(
                "missing fixture: CINNABAR_TEST_JAVA_PHYSICS_SERVER and CINNABAR_TEST_JAVA_CLIENT"
            );
            return;
        };
        if std::env::var("CINNABAR_TEST_JAVA_EULA_ACCEPTED").as_deref() != Ok("1") {
            eprintln!("missing fixture: explicit CINNABAR_TEST_JAVA_EULA_ACCEPTED=1");
            return;
        }
        let root = tempfile::tempdir().unwrap();
        let out = root.path().join("models.json");
        generate(Options {
            server: server.into(),
            client: client.into(),
            out: out.clone(),
            java: std::env::var_os("CINNABAR_TEST_JAVA_RUNTIME")
                .unwrap_or_else(|| "java".into())
                .into(),
            javac: "javac".into(),
            compiler_jar: std::env::var_os("CINNABAR_TEST_JAVA_COMPILER_JAR").map(Into::into),
            accept_eula: true,
        })
        .unwrap();
        let pin: Target = serde_json::from_str(TARGET).unwrap();
        let models = assets::entity::java::Models::load(
            &fs::read(out).unwrap(),
            &pin.entity_models_report.sha256,
            pin.entity_models_report.bytes,
        )
        .unwrap();
        assert_eq!(models.iter().count(), 9);
        for name in [
            "statue_standing",
            "statue_running",
            "statue_sitting",
            "statue_star",
        ] {
            assert!(models.get(name).is_some(), "missing {name}");
        }
        let pig = models.get("pig").unwrap();
        let head = pig.children.iter().find(|p| p.name == "head").unwrap();
        // Independently inspected pinned model factory: head + snout, two cubes in pixel space.
        assert_eq!(head.faces.len(), 12);
        assert_eq!(head.offset, [0., 12., -6.]);
        assert!(models.get("cold_pig").is_some() && models.get("player_slim").is_some());
    }
}
