//! Verified fluid facts preparation; the runtime consumes data, never a JVM.
use super::{
    java_physics::Options,
    java_runtime::{self, Runtime},
};
use serde::Deserialize;
use std::{error::Error, fs, io::Write};

pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java fluid preparation requires --accept-eula".into());
    }
    #[derive(Deserialize)]
    struct Target {
        fluid_render_report: java_runtime::Report,
        block_registry_report: Count,
    }
    #[derive(Deserialize)]
    struct Count {
        states: usize,
    }
    let target: Target = serde_json::from_str(java_runtime::TARGET)?;
    let runtime = Runtime::new(&options.server, &options.java)?;
    let output = runtime.path().join("fluids.json");
    runtime.export(
        "ExportFluids",
        include_str!("java/ExportFluids.java"),
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&output],
    )?;
    java_runtime::verify(
        &output,
        &target.fluid_render_report.sha256,
        target.fluid_render_report.bytes,
    )?;
    let bytes = fs::read(&output)?;
    assets::registry::java::fluids::Table::load(
        &bytes,
        &target.fluid_render_report.sha256,
        target.block_registry_report.states,
    )?;
    let parent = options
        .out
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(std::path::Path::new("."));
    fs::create_dir_all(parent)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(&bytes)?;
    file.as_file().sync_all()?;
    file.persist(&options.out)?;
    println!(
        "Verified Java fluid render facts: {}",
        options.out.display()
    );
    Ok(())
}
