//! Verified atomic installation and bounded official downloads for Java preparation.
use super::java_runtime::{Report, verify};
use std::{
    error::Error,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::Command,
    time::Duration,
};

pub(super) fn current(path: &Path, pin: &Report) -> bool {
    verify(path, &pin.sha256, pin.bytes).is_ok()
}

/// Never replaces the destination until the staged bytes match the pinned report.
pub(super) fn install(
    source: &Path,
    destination: &Path,
    pin: &Report,
) -> Result<(), Box<dyn Error>> {
    verify(source, &pin.sha256, pin.bytes)?;
    let mut staged =
        tempfile::NamedTempFile::new_in(destination.parent().ok_or("missing parent")?)?;
    std::io::copy(&mut fs::File::open(source)?, &mut staged)?;
    publish(staged, destination, pin)
}

fn publish(
    staged: tempfile::NamedTempFile,
    destination: &Path,
    pin: &Report,
) -> Result<(), Box<dyn Error>> {
    verify(staged.path(), &pin.sha256, pin.bytes)?;
    staged.as_file().sync_all()?;
    staged.persist(destination)?;
    Ok(())
}

pub(super) fn acquire(
    destination: &Path,
    supplied: Option<&Path>,
    url: &str,
    pin: &Report,
) -> Result<(), Box<dyn Error>> {
    if current(destination, pin) {
        return Ok(());
    }
    if let Some(source) = supplied {
        return install(source, destination, pin);
    }
    let mut staged =
        tempfile::NamedTempFile::new_in(destination.parent().ok_or("missing parent")?)?;
    let url = reqwest::Url::parse(url)?;
    if url.scheme() != "https" || url.host_str() != Some("piston-data.mojang.com") {
        return Err("Java archives must use the pinned official HTTPS download host".into());
    }
    println!("Downloading {}", url);
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?
        .block_on(async {
            let client = reqwest::Client::builder()
                .redirect(reqwest::redirect::Policy::none())
                .connect_timeout(Duration::from_secs(30))
                .timeout(Duration::from_secs(600))
                .build()?;
            let mut response = client.get(url).send().await?.error_for_status()?;
            if response.status() != reqwest::StatusCode::OK {
                return Err("official download did not return HTTP 200".into());
            }
            let mut total = 0u64;
            while let Some(chunk) = response.chunk().await? {
                total = total
                    .checked_add(chunk.len() as u64)
                    .ok_or("download size overflow")?;
                if total > pin.bytes {
                    return Err("Java archive exceeds pinned size".into());
                }
                staged.write_all(&chunk)?;
            }
            Ok::<_, Box<dyn Error>>(())
        })?;
    publish(staged, destination, pin)
}

pub(super) fn jdk(
    java: Option<PathBuf>,
    javac: Option<PathBuf>,
    major: u32,
) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    if java.is_some() || javac.is_some() {
        return checked(
            java.unwrap_or_else(|| "java".into()),
            javac.unwrap_or_else(|| "javac".into()),
            major,
        );
    }
    if let Some(home) = std::env::var_os("JAVA_HOME") {
        let home = PathBuf::from(home);
        if let Ok(pair) = checked(home.join("bin/java"), home.join("bin/javac"), major) {
            return Ok(pair);
        }
    }
    if let Ok(pair) = checked("java".into(), "javac".into(), major) {
        return Ok(pair);
    }
    // macOS may have a Homebrew JDK without registration in java_home.
    for (program, args, suffix) in [
        (
            "/usr/libexec/java_home",
            vec!["-v".to_owned(), major.to_string()],
            "",
        ),
        (
            "brew",
            vec!["--prefix".to_owned(), format!("openjdk@{major}")],
            "libexec/openjdk.jdk/Contents/Home",
        ),
    ] {
        if let Ok(output) = Command::new(program).args(args).output()
            && output.status.success()
            && let Ok(home) = std::str::from_utf8(&output.stdout)
        {
            let home = Path::new(home.trim()).join(suffix);
            if let Ok(pair) = checked(home.join("bin/java"), home.join("bin/javac"), major) {
                return Ok(pair);
            }
        }
    }
    Err(format!("JDK {major} is required to generate Java reports. Set JAVA_HOME to its installation directory, or on macOS install openjdk@{major} with Homebrew. A JRE alone is insufficient.").into())
}

fn checked(
    java: PathBuf,
    javac: PathBuf,
    major: u32,
) -> Result<(PathBuf, PathBuf), Box<dyn Error>> {
    for tool in [&java, &javac] {
        let output = Command::new(tool)
            .arg("-version")
            .output()
            .map_err(|e| format!("Cannot run {}: {e}", tool.display()))?;
        let version = format!(
            "{} {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        if !output.status.success() || version_major(&version) != Some(major) {
            return Err(format!("{} must be JDK {major}", tool.display()).into());
        }
    }
    Ok((java, javac))
}

fn version_major(text: &str) -> Option<u32> {
    text.lines().find_map(|line| {
        let line = line.trim();
        let value = line
            .strip_prefix("javac ")
            .or_else(|| line.strip_prefix("java version \""))
            .or_else(|| line.strip_prefix("openjdk version \""))?;
        value.split(['.', '-', '"', ' ']).next()?.parse().ok()
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_tool_versions_without_matching_warning_numbers() {
        assert_eq!(
            version_major("warning: 25 issues\nopenjdk version \"25.0.1\" 2025-10-21"),
            Some(25)
        );
        assert_eq!(version_major("java version \"21.0.1\""), Some(21));
        assert_eq!(version_major("javac 25-ea"), Some(25));
        assert_eq!(version_major("Unable to locate Java Runtime 25"), None);
    }
}
