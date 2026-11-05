//! Reproducible generation of the source-bundled native use-dispatch facts.
use super::{
    java_physics::Options,
    java_runtime::{Runtime, verify},
};
use std::{error::Error, fs, io::Write, path::Path};
pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java use-rule preparation requires explicit --accept-eula".into());
    }
    let target: serde_json::Value =
        serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))?;
    let runtime = Runtime::new(&options.server, &options.java)?;
    let report = runtime.path().join("use-rules.json");
    runtime.export(
        "ExportUseRules",
        include_str!("java/ExportUseRules.java"),
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&report],
    )?;
    let pin = &target["use_rules_report"];
    verify(
        &report,
        pin["sha256"].as_str().ok_or("missing use-rule hash")?,
        pin["bytes"].as_u64().ok_or("missing use-rule length")?,
    )?;
    let bytes = fs::read(&report)?;
    assets::registry::java::use_rules::Table::load(&bytes)?;
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
        "Verified native Java use-rule facts: {}",
        options.out.display()
    );
    Ok(())
}
