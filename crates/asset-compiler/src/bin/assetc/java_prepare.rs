//! Complete, resumable preparation of the pinned native Java runtime directory.
use super::{java_entities, java_interaction, java_physics, java_runtime};
use clap::Args;
use serde::Deserialize;
use std::{error::Error, fs, path::PathBuf};

#[path = "java_prepare_io.rs"]
mod io;

#[derive(Debug, Args)]
pub(super) struct Options {
    /// Runtime directory; defaults to .local/assets/java/<pinned-version>.
    #[arg(long)]
    out: Option<PathBuf>,
    /// Download cache; defaults to .local/cache/java/<pinned-version>.
    #[arg(long)]
    cache: Option<PathBuf>,
    /// Import an already downloaded official server instead of fetching it.
    #[arg(long)]
    server: Option<PathBuf>,
    /// Import an already downloaded official client archive instead of fetching it.
    #[arg(long)]
    client: Option<PathBuf>,
    /// Override JDK discovery (JAVA_HOME, PATH, macOS java_home, Homebrew).
    #[arg(long)]
    java: Option<PathBuf>,
    #[arg(long)]
    javac: Option<PathBuf>,
    /// Accept https://www.minecraft.net/eula before downloading or processing game files.
    #[arg(long)]
    accept_eula: bool,
}

#[derive(Deserialize)]
struct Target {
    game_version: String,
    java_runtime_major: u32,
    server_url: String,
    server_sha256: String,
    server_bytes: u64,
    client_assets: Client,
    block_registry_report: Report,
    physics_report: Report,
    interaction_report: Report,
    entity_models_report: Report,
}

#[derive(Deserialize)]
struct Client {
    url: String,
    #[serde(flatten)]
    integrity: java_runtime::Report,
}

#[derive(Deserialize)]
struct Report {
    path: PathBuf,
    #[serde(flatten)]
    integrity: java_runtime::Report,
}

impl Report {
    fn name(&self) -> &std::ffi::OsStr {
        self.path.file_name().expect("pinned report has a filename")
    }
}

impl Target {
    fn inputs(&self) -> Vec<(&std::ffi::OsStr, &java_runtime::Report)> {
        std::iter::once((
            std::ffi::OsStr::new("client.jar"),
            &self.client_assets.integrity,
        ))
        .chain(
            [
                &self.block_registry_report,
                &self.physics_report,
                &self.interaction_report,
                &self.entity_models_report,
            ]
            .map(|r| (r.name(), &r.integrity)),
        )
        .collect()
    }
}

pub(super) fn prepare(options: Options) -> Result<(), Box<dyn Error>> {
    prepare_target(options, serde_json::from_str(java_runtime::TARGET)?)
}

fn prepare_target(options: Options, pin: Target) -> Result<(), Box<dyn Error>> {
    let out = options
        .out
        .unwrap_or_else(|| PathBuf::from(".local/assets/java").join(&pin.game_version));
    // Imports may already be the intended client.jar, but must not alias another output.
    for (name, _) in pin.inputs() {
        for input in [
            options.server.as_deref(),
            options.java.as_deref(),
            options.javac.as_deref(),
        ]
        .into_iter()
        .flatten()
        {
            super::validate_output_bundle(&out.join(name), input)?;
        }
        if name != "client.jar"
            && let Some(input) = options.client.as_deref()
        {
            super::validate_output_bundle(&out.join(name), input)?;
        }
    }
    let pending = pin
        .inputs()
        .into_iter()
        .filter(|(name, expected)| !io::current(&out.join(name), expected))
        .map(|(name, _)| name.to_string_lossy().into_owned())
        .collect::<Vec<_>>();
    if pending.is_empty() {
        println!(
            "All Java {} runtime inputs verified: {}",
            pin.game_version,
            out.display()
        );
        return Ok(());
    }
    println!("Java runtime inputs to prepare: {}", pending.join(", "));
    if !options.accept_eula {
        return Err("Java preparation requires EULA acceptance: make java-assets JAVA_ACCEPT_EULA=1 (https://www.minecraft.net/eula)".into());
    }
    // No JVM is needed when only the resource archive is missing.
    let reports_needed = pending.iter().any(|name| name != "client.jar");
    let jdk = if reports_needed {
        Some(io::jdk(
            options.java,
            options.javac,
            pin.java_runtime_major,
        )?)
    } else {
        None
    };
    fs::create_dir_all(&out)?;
    let out = out.canonicalize()?;
    let client = out.join("client.jar");
    io::acquire(
        &client,
        options.client.as_deref(),
        &pin.client_assets.url,
        &pin.client_assets.integrity,
    )?;
    if let Some((java, javac)) = jdk {
        let cache = options
            .cache
            .unwrap_or_else(|| PathBuf::from(".local/cache/java").join(&pin.game_version));
        fs::create_dir_all(&cache)?;
        let server = cache.canonicalize()?.join("server.jar");
        io::acquire(
            &server,
            options.server.as_deref(),
            &pin.server_url,
            &java_runtime::Report {
                sha256: pin.server_sha256.clone(),
                bytes: pin.server_bytes,
            },
        )?;
        let blocks = out.join(pin.block_registry_report.name());
        if !io::current(&blocks, &pin.block_registry_report.integrity) {
            let runtime = java_runtime::Runtime::new(&server, &java)?;
            let generated = runtime.reports()?.join(&pin.block_registry_report.path);
            io::install(&generated, &blocks, &pin.block_registry_report.integrity)?;
        }
        let physics = out.join(pin.physics_report.name());
        if !io::current(&physics, &pin.physics_report.integrity) {
            java_physics::generate(java_physics::Options {
                server: server.clone(),
                out: physics,
                java: java.clone(),
                javac: javac.clone(),
                compiler_jar: None,
                accept_eula: true,
            })?;
        }
        let interaction = out.join(pin.interaction_report.name());
        if !io::current(&interaction, &pin.interaction_report.integrity) {
            java_interaction::generate(java_interaction::Options {
                server: server.clone(),
                out: interaction,
                java: java.clone(),
                javac: javac.clone(),
                compiler_jar: None,
                accept_eula: true,
            })?;
        }
        let models = out.join(pin.entity_models_report.name());
        if !io::current(&models, &pin.entity_models_report.integrity) {
            java_entities::generate(java_entities::Options {
                server,
                client,
                out: models,
                java,
                javac,
                compiler_jar: None,
                accept_eula: true,
            })?;
        }
    }
    for (name, expected) in pin.inputs() {
        java_runtime::verify(&out.join(name), &expected.sha256, expected.bytes)?;
    }
    println!(
        "All Java {} runtime inputs verified: {}",
        pin.game_version,
        out.display()
    );
    Ok(())
}

#[cfg(test)]
#[path = "java_prepare_tests.rs"]
mod tests;
