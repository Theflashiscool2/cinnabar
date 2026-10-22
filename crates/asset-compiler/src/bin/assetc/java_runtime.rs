//! Verified offline Java data preparation shared by collision and entity exporters.
use serde::Deserialize;
use std::{
    error::Error,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};

pub(super) const TARGET: &str = include_str!("../../../../java-protocol/data/target.json");
#[derive(Deserialize)]
struct Target {
    game_version: String,
    java_runtime_major: u32,
    server_sha256: String,
    server_bytes: u64,
    client_assets: Report,
}
#[derive(Deserialize)]
pub(super) struct Report {
    pub sha256: String,
    pub bytes: u64,
}

pub(super) struct Runtime {
    work: tempfile::TempDir,
    java: PathBuf,
    jars: Vec<PathBuf>,
    target: Target,
}
impl Runtime {
    /// Verifies executable archives before loading them. Extraction uses generated flat names.
    pub fn new(server: &Path, java: &Path) -> Result<Self, Box<dyn Error>> {
        let target: Target = serde_json::from_str(TARGET)?;
        verify(server, &target.server_sha256, target.server_bytes)?;
        let java = if java.components().count() > 1 {
            java.canonicalize()?
        } else {
            java.to_owned()
        };
        let work = tempfile::tempdir()?;
        let mut archive = zip::ZipArchive::new(fs::File::open(server)?)?;
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
                let jar = work.path().join(format!("library-{}.jar", jars.len()));
                let mut entry = archive.by_name(&format!("META-INF/{folder}/{path}"))?;
                let length = entry.size();
                std::io::copy(&mut entry, &mut fs::File::create(&jar)?)?;
                verify(&jar, hash, length)?;
                jars.push(jar);
            }
        }
        Ok(Self {
            work,
            java,
            jars,
            target,
        })
    }
    pub fn path(&self) -> &Path {
        self.work.path()
    }

    /// Adds the client model factories as verified data inputs, without starting the client.
    pub fn add_client(&mut self, client: &Path) -> Result<(), Box<dyn Error>> {
        let pinned = &self.target.client_assets;
        verify(client, &pinned.sha256, pinned.bytes)?;
        let jar = self.work.path().join("client.jar");
        fs::copy(client, &jar)?;
        verify(&jar, &pinned.sha256, pinned.bytes)?;
        self.jars.push(jar);
        Ok(())
    }

    /// Compiles original exporter code and runs it in an isolated temporary working directory.
    pub fn export(
        &self,
        name: &str,
        code: &str,
        javac: &Path,
        compiler_jar: Option<&Path>,
        outputs: &[&Path],
    ) -> Result<(), Box<dyn Error>> {
        let source = self.work.path().join(format!("{name}.java"));
        fs::write(&source, code)?;
        let classpath = std::env::join_paths(&self.jars)?;
        let release = self.target.java_runtime_major.to_string();
        let mut compiler = if let Some(jar) = compiler_jar {
            let mut command = Command::new(&self.java);
            command
                .args(["-Xmx512m", "-jar"])
                .arg(jar)
                .args(["-source", &release, "-target", &release]);
            command
        } else {
            let mut command = Command::new(javac);
            command.args(["--release", &release]);
            command
        };
        let result = compiler
            .arg("-cp")
            .arg(&classpath)
            .arg("-d")
            .arg(self.work.path())
            .arg(&source)
            .status()?;
        if !result.success() {
            return Err(format!("Java {name} compilation failed: {result}").into());
        }
        let classpath = std::env::join_paths(
            std::iter::once(self.work.path()).chain(self.jars.iter().map(PathBuf::as_path)),
        )?;
        let result = Command::new(&self.java)
            .arg("-Xmx512m")
            .arg("-cp")
            .arg(classpath)
            .arg(name)
            .args(outputs)
            .current_dir(self.work.path())
            .status()?;
        if !result.success() {
            return Err(format!("Java {name} failed: {result}").into());
        }
        Ok(())
    }
}

pub(super) fn verify(path: &Path, sha256: &str, bytes: u64) -> Result<(), Box<dyn Error>> {
    if fs::metadata(path)?.len() != bytes || assets::vanilla_pack::sha256_file(path)? != sha256 {
        return Err(format!("Java source hash/length mismatch: {}", path.display()).into());
    }
    Ok(())
}
