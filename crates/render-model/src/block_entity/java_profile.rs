//! Native 26.3 skull profile extraction. Resolution and downloads belong to the host.
use world::NbtValue;

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Profile {
    /// Absent profile uses the native unprofiled head default, distinct from a nil UUID profile.
    pub present: bool,
    pub id: Option<[u8; 16]>,
    pub name: Option<Box<str>>,
    pub textures: Option<Box<str>>,
    pub has_properties: bool,
    pub texture_patch: Option<Box<str>>,
}
impl Profile {
    /// Reads ResolvableProfile.CODEC's list-shaped properties without interpreting their
    /// contents as local paths. Resource identifiers are validated by the asset owner.
    pub fn from_block_entity(data: Option<&NbtValue>) -> Result<Self, &'static str> {
        let Some(data) = data else {
            return Ok(Self::default());
        };
        let NbtValue::Compound(root) = data else {
            return Err("Java head block entity compound expected");
        };
        let Some(profile) = root.get("profile") else {
            return Ok(Self::default());
        };
        if let NbtValue::String(name) = profile {
            return Ok(Self {
                present: true,
                name: Some(name.clone()),
                ..Default::default()
            });
        }
        let NbtValue::Compound(profile) = profile else {
            return Err("Java 26.3 head profile compound expected");
        };
        // Cape, elytra and arm-model patches do not change a placed head's geometry.
        for key in ["texture", "cape", "elytra", "model"] {
            if profile.get(key).is_some() && profile.string(key).is_none() {
                return Err("Java head skin patch must be a string");
            }
        }
        let texture_patch = profile.string("texture").map(Box::from);
        let id = match profile.get("id") {
            None => None,
            Some(NbtValue::IntArray(words)) if words.len() == 4 => {
                let mut id = [0; 16];
                for (chunk, word) in id.chunks_exact_mut(4).zip(words) {
                    chunk.copy_from_slice(&word.to_be_bytes());
                }
                Some(id)
            }
            _ => return Err("Java head profile UUID must contain four integers"),
        };
        let name = match profile.get("name") {
            None => None,
            Some(NbtValue::String(name)) => Some(name.clone()),
            _ => return Err("Java head profile name must be a string"),
        };
        let mut textures = None;
        let mut has_properties = false;
        if let Some(properties) = profile.get("properties") {
            let NbtValue::List(properties) = properties else {
                return Err("Java 26.3 profile properties must be a list");
            };
            has_properties = !properties.is_empty();
            for property in properties {
                let NbtValue::Compound(property) = property else {
                    return Err("Java profile property compound expected");
                };
                let Some(key) = property.string("name") else {
                    return Err("Java profile property name missing");
                };
                let Some(value) = property.string("value") else {
                    return Err("Java profile property value missing");
                };
                if property.get("signature").is_some() && property.string("signature").is_none() {
                    return Err("Java profile signature must be a string");
                }
                if key == "textures" && textures.is_none() {
                    textures = Some(value.into());
                }
            }
        }
        Ok(Self {
            present: true,
            id,
            name,
            textures,
            has_properties,
            texture_patch,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use world::NbtCompound;
    fn wrapped(profile: NbtCompound) -> NbtValue {
        let mut root = NbtCompound::default();
        root.insert("profile", NbtValue::Compound(profile));
        NbtValue::Compound(root)
    }
    #[test]
    fn native_codec_uuid_and_list_properties_preserve_opaque_values() {
        // Independent InspectHeadProfile output from the verified 26.3 runtime.
        let mut p = NbtCompound::default();
        p.insert(
            "id",
            NbtValue::IntArray(vec![305419896, 305419896, -1698898192, 305419896]),
        );
        p.insert("name", NbtValue::String("Fixture".into()));
        let mut property = NbtCompound::default();
        property.insert("name", NbtValue::String("textures".into()));
        property.insert("value", NbtValue::String("opaque-texture-value".into()));
        property.insert("signature", NbtValue::String("fixture-signature".into()));
        p.insert(
            "properties",
            NbtValue::List(vec![NbtValue::Compound(property)]),
        );
        let parsed = Profile::from_block_entity(Some(&wrapped(p))).unwrap();
        assert!(parsed.present);
        assert_eq!(
            parsed.id,
            Some([
                0x12, 0x34, 0x56, 0x78, 0x12, 0x34, 0x56, 0x78, 0x9a, 0xbc, 0xde, 0xf0, 0x12, 0x34,
                0x56, 0x78
            ])
        );
        assert_eq!(parsed.name.as_deref(), Some("Fixture"));
        assert_eq!(parsed.textures.as_deref(), Some("opaque-texture-value"));
    }
    #[test]
    fn absent_profile_is_distinct_from_malformed_or_unsupported_data() {
        assert_eq!(
            Profile::from_block_entity(None).unwrap(),
            Profile::default()
        );
        assert!(Profile::from_block_entity(Some(&NbtValue::String("path".into()))).is_err());
        for (key, value) in [
            ("id", NbtValue::IntArray(vec![1, 2, 3])),
            ("properties", NbtValue::Compound(Default::default())),
            ("texture", NbtValue::Int(12)),
        ] {
            let mut p = NbtCompound::default();
            p.insert(key, value);
            assert!(Profile::from_block_entity(Some(&wrapped(p))).is_err());
        }
    }
}

#[cfg(test)]
mod compact_tests {
    use super::*;
    #[test]
    fn native_compact_name_decodes_to_unresolved_profile() {
        let mut root = world::NbtCompound::default();
        root.insert("profile", NbtValue::String("Fixture".into()));
        let profile = Profile::from_block_entity(Some(&NbtValue::Compound(root))).unwrap();
        assert!(profile.present);
        assert_eq!(profile.name.as_deref(), Some("Fixture"));
        assert_eq!(profile.id, None);
        assert_eq!(profile.textures, None);
    }
}
