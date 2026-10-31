//! Verifies and exports native outline shapes independently of movement collision shapes.
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
    /// Runtime interaction.bin, alongside physics.bin and blocks.json.
    #[arg(long)]
    pub out: PathBuf,
    #[arg(long, default_value = "java")]
    pub java: PathBuf,
    #[arg(long, default_value = "javac", conflicts_with = "compiler_jar")]
    pub javac: PathBuf,
    #[arg(long)]
    pub compiler_jar: Option<PathBuf>,
    /// Explicit acceptance of the Minecraft EULA for processing supplied game assets.
    #[arg(long)]
    pub(super) accept_eula: bool,
}
#[derive(Deserialize)]
struct Target {
    interaction_report: Report,
    block_registry_report: Registry,
}
#[derive(Deserialize)]
struct Registry {
    states: usize,
}
pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java interaction preparation requires explicit --accept-eula".into());
    }
    let pin: Target = serde_json::from_str(TARGET)?;
    let runtime = Runtime::new(&options.server, &options.java)?;
    let report = runtime.path().join("interaction.bin");
    runtime.export(
        "ExportInteraction",
        include_str!("java/ExportInteraction.java"),
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&report],
    )?;
    verify(
        &report,
        &pin.interaction_report.sha256,
        pin.interaction_report.bytes,
    )?;
    let bytes = fs::read(&report)?;
    assets::registry::java::interaction::Table::load(
        &bytes,
        &pin.interaction_report.sha256,
        pin.block_registry_report.states,
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
        "Verified native Java interaction shapes: {}",
        options.out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn input_aliases_are_rejected_before_export() {
        use clap::Parser;
        for source in ["server.jar", "compiler.jar"] {
            let parsed = crate::cli::Cli::try_parse_from([
                "assetc",
                "java-interaction",
                "--server",
                "server.jar",
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
    }
}
