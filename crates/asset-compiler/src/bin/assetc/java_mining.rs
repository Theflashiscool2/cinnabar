//! Reproducible generation of the source-bundled native breaking facts.
use super::{
    java_physics::Options,
    java_runtime::{Runtime, verify},
};
use std::{error::Error, fs, io::Write, path::Path};
pub(super) fn generate(options: Options) -> Result<(), Box<dyn Error>> {
    if !options.accept_eula {
        return Err("Java mining preparation requires explicit --accept-eula".into());
    }
    let target: serde_json::Value =
        serde_json::from_str(include_str!("../../../../java-protocol/data/target.json"))?;
    let runtime = Runtime::new(&options.server, &options.java)?;
    let report = runtime.path().join("mining.json");
    runtime.export(
        "ExportMining",
        include_str!("java/ExportMining.java"),
        &options.javac,
        options.compiler_jar.as_deref(),
        &[&report],
    )?;
    let pin = &target["mining_report"];
    verify(
        &report,
        pin["sha256"].as_str().ok_or("missing mining hash")?,
        pin["bytes"].as_u64().ok_or("missing mining length")?,
    )?;
    let bytes = fs::read(&report)?;
    assets::registry::java::mining::Table::load(&bytes)?;
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
        "Verified native Java mining facts: {}",
        options.out.display()
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn options(root: &Path) -> super::super::java_physics::Options {
        super::super::java_physics::Options {
            server: root.join("invalid.jar"),
            out: root.join("existing.json"),
            java: root.join("must-not-execute-java"),
            javac: root.join("must-not-execute-javac"),
            compiler_jar: None,
            accept_eula: false,
        }
    }
    #[test]
    fn eula_and_verified_archive_precede_execution_and_never_overwrite_good_output() {
        let r = tempfile::tempdir().unwrap();
        let mut o = options(r.path());
        fs::write(&o.out, b"retained").unwrap();
        assert!(
            generate(o)
                .unwrap_err()
                .to_string()
                .contains("--accept-eula")
        );
        assert_eq!(
            fs::read(r.path().join("existing.json")).unwrap(),
            b"retained"
        );
        o = options(r.path());
        o.accept_eula = true;
        fs::write(&o.server, b"unverified archive").unwrap();
        assert!(generate(o).is_err());
        assert_eq!(
            fs::read(r.path().join("existing.json")).unwrap(),
            b"retained"
        );
    }
}
