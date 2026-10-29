//! Native Java resources use the same bundle root as the shared carriers.
use super::{InstallLayout, LayoutError};
use std::path::PathBuf;

impl InstallLayout {
    /// Read-only, version-pinned runtime inputs prepared on the host, never in the app's cwd.
    pub fn java_assets(&self) -> PathBuf {
        self.resource_root
            .join("assets/java")
            .join(java_protocol::target::GAME_VERSION)
    }
}

/// Preserves the explicit developer override while resolving packaged installs through their host.
pub fn discover_java_assets() -> Result<PathBuf, LayoutError> {
    match std::env::var_os("CINNABAR_JAVA_ASSETS") {
        Some(root) => Ok(PathBuf::from(root)),
        None => Ok(InstallLayout::discover()?.java_assets()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn iphone_java_assets_stay_in_bundle_when_container_changes() {
        let resources = PathBuf::from("/Bundle/Cinnabar.app/local");
        for container in ["/Container/A", "/Container/B"] {
            let layout = InstallLayout::ios(
                resources.clone(),
                Path::new(container),
                "/Bundle/Cinnabar.app/Cinnabar".into(),
            );
            assert_eq!(
                layout.java_assets(),
                resources
                    .join("assets/java")
                    .join(java_protocol::target::GAME_VERSION)
            );
            assert!(
                !layout
                    .java_assets()
                    .starts_with(layout.transient_runtime_root())
            );
        }
    }
}
