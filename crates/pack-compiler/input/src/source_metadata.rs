//! Host filesystem metadata is not resource-pack content.

use std::path::Path;

/// Identifies Finder metadata and AppleDouble sidecars created on non-Apple filesystems.
/// Match only reserved basenames; ordinary dotfiles and malformed assets remain inputs.
pub fn is_source_metadata(path: &Path) -> bool {
    path.file_name().is_some_and(|name| {
        let name = name.as_encoded_bytes();
        name.starts_with(b"._") || name == b".DS_Store"
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn excludes_metadata_without_hiding_real_or_hidden_assets() {
        for name in ["._allay", "fogs/._bamboo.json", ".DS_Store"] {
            assert!(is_source_metadata(Path::new(name)), "{name}");
        }
        for name in [
            "allay",
            "bamboo.json",
            ".custom.json",
            "_allay.png",
            "a._b.json",
            "._parent/real.json",
        ] {
            assert!(!is_source_metadata(Path::new(name)), "{name}");
        }
    }
}
