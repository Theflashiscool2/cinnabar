use super::*;
use clap::Parser;
use sha2::{Digest, Sha256};

fn options(root: &std::path::Path) -> Options {
    Options {
        out: Some(root.join("runtime")),
        cache: Some(root.join("cache")),
        server: None,
        client: None,
        java: Some(root.join("no-java")),
        javac: Some(root.join("no-javac")),
        accept_eula: true,
    }
}

fn fixture(root: &std::path::Path) -> Target {
    let mut value: serde_json::Value = serde_json::from_str(java_runtime::TARGET).unwrap();
    let data = b"verified fixture data, never executed";
    for key in [
        "client_assets",
        "block_registry_report",
        "physics_report",
        "interaction_report",
        "entity_models_report",
        "fluid_render_report",
    ] {
        value[key]["bytes"] = (data.len() as u64).into();
        value[key]["sha256"] = format!("{:x}", Sha256::digest(data)).into();
    }
    let pin: Target = serde_json::from_value(value).unwrap();
    fs::create_dir_all(root.join("runtime")).unwrap();
    for (name, _) in pin.inputs() {
        fs::write(root.join("runtime").join(name), data).unwrap();
    }
    pin
}

#[test]
fn complete_runtime_needs_neither_network_nor_jdk_nor_new_consent() {
    let root = tempfile::tempdir().unwrap();
    let pin = fixture(root.path());
    let mut opts = options(root.path());
    opts.accept_eula = false;
    prepare_target(opts, pin).unwrap();
    assert!(!root.path().join("cache").exists());
}

#[test]
fn missing_client_is_installed_alongside_reports_without_a_jdk() {
    let root = tempfile::tempdir().unwrap();
    let pin = fixture(root.path());
    let client = root.path().join("runtime/client.jar");
    let supplied = root.path().join("downloaded-client.jar");
    fs::rename(&client, &supplied).unwrap();
    let mut opts = options(root.path());
    opts.client = Some(supplied.clone());
    prepare_target(opts, pin).unwrap();
    assert_eq!(fs::read(&client).unwrap(), fs::read(&supplied).unwrap());
    assert!(!root.path().join("cache").exists());
}

#[test]
fn corrupted_import_preserves_existing_output_and_leaves_no_staging_file() {
    let root = tempfile::tempdir().unwrap();
    let pin = fixture(root.path());
    let entries = || {
        fs::read_dir(root.path().join("runtime"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect::<std::collections::BTreeSet<_>>()
    };
    let original_entries = entries();
    let client = root.path().join("runtime/client.jar");
    fs::write(&client, "previous incomplete input").unwrap();
    let supplied = root.path().join("bad.jar");
    fs::write(&supplied, "invalid").unwrap();
    let mut opts = options(root.path());
    opts.client = Some(supplied);
    assert!(
        prepare_target(opts, pin)
            .unwrap_err()
            .to_string()
            .contains("hash/length mismatch")
    );
    assert_eq!(
        fs::read_to_string(client).unwrap(),
        "previous incomplete input"
    );
    assert_eq!(entries(), original_entries);
}

#[test]
fn consent_is_checked_before_creating_outputs_or_running_tools() {
    let root = tempfile::tempdir().unwrap();
    let mut opts = options(root.path());
    opts.accept_eula = false;
    assert!(
        prepare(opts)
            .unwrap_err()
            .to_string()
            .contains("JAVA_ACCEPT_EULA=1")
    );
    assert_eq!(fs::read_dir(root.path()).unwrap().count(), 0);
}

#[test]
fn invalid_report_requires_jdk_even_if_archive_is_current() {
    let root = tempfile::tempdir().unwrap();
    let pin = fixture(root.path());
    fs::write(root.path().join("runtime/physics.bin"), "bad").unwrap();
    assert!(
        prepare_target(options(root.path()), pin)
            .unwrap_err()
            .to_string()
            .contains("no-java")
    );
    assert!(!root.path().join("cache").exists());
}

#[test]
fn cli_exposes_complete_setup_and_local_imports() {
    assert!(
        crate::cli::Cli::try_parse_from([
            "assetc",
            "java-prepare",
            "--server",
            "server.jar",
            "--client",
            "client.jar",
            "--out",
            "runtime",
            "--accept-eula"
        ])
        .is_ok()
    );
}

#[test]
fn target_runtime_inventory_includes_all_six_inputs() {
    let pin: Target = serde_json::from_str(java_runtime::TARGET).unwrap();
    let names = pin
        .inputs()
        .iter()
        .map(|(n, _)| n.to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        names,
        [
            "client.jar",
            "blocks.json",
            "physics.bin",
            "interaction.bin",
            "models.json",
            "fluids.json"
        ]
        .map(String::from)
        .into()
    );
}

#[test]
fn imports_cannot_be_overwritten_by_a_different_report() {
    let root = tempfile::tempdir().unwrap();
    let pin = fixture(root.path());
    let source = root.path().join("runtime/physics.bin");
    let before = fs::read(&source).unwrap();
    let mut opts = options(root.path());
    opts.client = Some(source.clone());
    assert!(
        prepare_target(opts, pin)
            .unwrap_err()
            .to_string()
            .contains("distinct files")
    );
    assert_eq!(fs::read(source).unwrap(), before);
}
