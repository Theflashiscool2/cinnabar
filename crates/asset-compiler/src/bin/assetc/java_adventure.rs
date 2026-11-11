//! Reproducible generation of the source-bundled native Adventure permission facts.
use super::{
    java_physics::Options,
    java_runtime::{Runtime, verify},
};
use std::{error::Error, fs, io::Write, path::Path};
pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java Adventure-permission preparation requires explicit --accept-eula".into());
    }
    let target: serde_json::Value =
        serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))?;
    use sha2::{Digest, Sha256};
    let code = include_bytes!("java/ExportAdventure.java");
    if Some(format!("{:x}", Sha256::digest(code)).as_str())
        != target["adventure_report"]["source_sha256"].as_str()
    {
        return Err("Native Adventure exporter source does not match its pin".into());
    }
    let runtime = Runtime::new(&options.server, &options.java)?;
    let report = runtime.path().join("Adventure-permissions.json");
    let source = runtime.path().join("ExportAdventure.java");
    runtime.export(
        "ExportAdventure",
        include_str!("java/ExportAdventure.java"),
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&report, &source],
    )?;
    let pin = &target["adventure_report"];
    verify(
        &report,
        pin["sha256"]
            .as_str()
            .ok_or("missing Adventure-permission hash")?,
        pin["bytes"]
            .as_u64()
            .ok_or("missing Adventure-permission length")?,
    )?;
    let bytes = fs::read(&report)?;
    assets::registry::java::adventure::Table::load(&bytes)?;
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
        "Verified native Java Adventure-permission facts: {}",
        options.out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn adventure_exporter_source_matches_manifest_provenance() {
        use sha2::{Digest, Sha256};
        let target: serde_json::Value =
            serde_json::from_str(super::super::java_runtime::TARGET).unwrap();
        assert_eq!(
            format!(
                "{:x}",
                Sha256::digest(include_bytes!("java/ExportAdventure.java"))
            ),
            target["adventure_report"]["source_sha256"]
                .as_str()
                .unwrap()
        );
    }
}
