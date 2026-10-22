//! Native model hierarchy exported from the pinned Java factories at asset-preparation time.
//! Data stays in Java pixel/model space until the render-model admission boundary.
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Java entity models do not match the pinned report")]
    Hash,
    #[error("Java entity model JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("invalid Java entity models: {0}")]
    Invalid(&'static str),
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Face {
    pub normal: [f32; 3],
    /// Pixel XYZ and normalized UV, preserving the factory's face winding.
    pub vertices: [[f32; 5]; 4],
}
#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Part {
    pub name: String,
    pub offset: [f32; 3],
    pub rotation: [f32; 3],
    pub scale: [f32; 3],
    pub visible: bool,
    pub skip_draw: bool,
    pub faces: Vec<Face>,
    pub children: Vec<Part>,
}
#[derive(Debug)]
pub struct Models {
    models: BTreeMap<String, Part>,
    digest: [u8; 32],
}
impl Models {
    /// Checks provenance before allocating a parsed hierarchy. The pin owns the expected size.
    pub fn load(bytes: &[u8], expected_sha256: &str, expected_bytes: u64) -> Result<Self, Error> {
        let digest = Sha256::digest(bytes);
        if bytes.len() as u64 != expected_bytes || format!("{digest:x}") != expected_sha256 {
            return Err(Error::Hash);
        }
        let models: BTreeMap<String, Part> = serde_json::from_slice(bytes)?;
        if models.is_empty() {
            return Err(Error::Invalid("empty model report"));
        }
        for (name, root) in &models {
            if name.is_empty() || root.name != "root" {
                return Err(Error::Invalid("model identity"));
            }
            root.validate()?;
        }
        Ok(Self {
            models,
            digest: digest.into(),
        })
    }
    pub fn get(&self, name: &str) -> Option<&Part> {
        self.models.get(name)
    }
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Part)> {
        self.models.iter().map(|(name, part)| (name.as_str(), part))
    }
    pub fn digest(&self) -> [u8; 32] {
        self.digest
    }
}
impl Part {
    fn validate(&self) -> Result<(), Error> {
        if self.name.is_empty()
            || !self
                .offset
                .iter()
                .chain(&self.rotation)
                .chain(&self.scale)
                .all(|v| v.is_finite())
        {
            return Err(Error::Invalid("part name or transform"));
        }
        for face in &self.faces {
            let length = face.normal.into_iter().map(|n| n * n).sum::<f32>();
            if !length.is_finite()
                || (length - 1.).abs() > 1e-5
                || !face.vertices.iter().flatten().all(|v| v.is_finite())
            {
                return Err(Error::Invalid("face coordinates or normal"));
            }
        }
        let mut names = BTreeSet::new();
        for child in &self.children {
            if !names.insert(&child.name) {
                return Err(Error::Invalid("duplicate child name"));
            }
            child.validate()?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_wrong_source_and_invalid_hierarchy_without_fallback_models() {
        let base = serde_json::json!({"one":{"name":"root","offset":[0,0,0],"rotation":[0,0,0],"scale":[1,1,1],"visible":true,"skip_draw":false,"faces":[],"children":[]}});
        let check = |v: &serde_json::Value| {
            let bytes = serde_json::to_vec(v).unwrap();
            Models::load(
                &bytes,
                &format!("{:x}", Sha256::digest(&bytes)),
                bytes.len() as u64,
            )
        };
        assert!(check(&base).unwrap().get("one").is_some());
        assert!(matches!(
            Models::load(b"not JSON", "wrong", 8),
            Err(Error::Hash)
        ));
        let mut duplicates = base.clone();
        duplicates["one"]["children"] = serde_json::json!([base["one"], base["one"]]);
        assert!(matches!(
            check(&duplicates),
            Err(Error::Invalid("duplicate child name"))
        ));
        let mut invalid = base.clone();
        invalid["one"]["faces"] = serde_json::json!([{"normal":[0,0,0],"vertices":[[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0],[0,0,0,0,0]]}]);
        assert!(matches!(
            check(&invalid),
            Err(Error::Invalid("face coordinates or normal"))
        ));
    }
}
