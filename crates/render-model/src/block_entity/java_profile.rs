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
    /// Adapts the wire item component and persistent codec to the same head descriptor.
    /// Texture properties stay opaque here; only the shell may resolve public resources.
    pub fn from_item(value: &serde_json::Value) -> Result<Self, &'static str> {
        if value.is_null() {
            return Ok(Self::default());
        }
        if let Some(name) = value.as_str() {
            return Ok(Self {
                present: true,
                name: Some(name.into()),
                ..Default::default()
            });
        }
        let object = value
            .as_object()
            .ok_or("Java item profile object expected")?;
        let mut profile = world::NbtCompound::default();
        for (field, key) in [
            ("name", "name"),
            ("texture", "texture"),
            ("body", "texture"),
        ] {
            if let Some(v) = object.get(field).filter(|v| !v.is_null()) {
                profile.insert(
                    key,
                    NbtValue::String(
                        v.as_str()
                            .ok_or("Java item profile string expected")?
                            .into(),
                    ),
                );
            }
        }
        if let Some(id) = object.get("id").filter(|v| !v.is_null()) {
            let words = if let Some(text) = id.as_str() {
                let text = text.replace('-', "");
                if text.len() != 32 || !text.bytes().all(|c| c.is_ascii_hexdigit()) {
                    return Err("Java item profile UUID invalid");
                }
                (0..4)
                    .map(|i| {
                        u32::from_str_radix(&text[i * 8..i * 8 + 8], 16)
                            .map(|n| n as i32)
                            .map_err(|_| "Java item profile UUID invalid")
                    })
                    .collect::<Result<Vec<_>, _>>()?
            } else {
                id.as_array()
                    .filter(|a| a.len() == 4)
                    .ok_or("Java item profile UUID invalid")?
                    .iter()
                    .map(|v| {
                        v.as_i64()
                            .and_then(|n| i32::try_from(n).ok())
                            .ok_or("Java item profile UUID invalid")
                    })
                    .collect::<Result<Vec<_>, _>>()?
            };
            profile.insert("id", NbtValue::IntArray(words.into()));
        }
        if let Some(v) = object.get("properties").filter(|v| !v.is_null()) {
            let mut properties = Vec::new();
            for p in v
                .as_array()
                .ok_or("Java item profile properties list expected")?
            {
                let mut property = world::NbtCompound::default();
                for key in ["name", "value", "signature"] {
                    if let Some(v) = p.get(key).filter(|v| !v.is_null()) {
                        property.insert(
                            key,
                            NbtValue::String(
                                v.as_str()
                                    .ok_or("Java item profile property string expected")?
                                    .into(),
                            ),
                        );
                    }
                }
                properties.push(NbtValue::Compound(property));
            }
            profile.insert("properties", NbtValue::List(properties.into()));
        }
        let mut root = world::NbtCompound::default();
        root.insert("profile", NbtValue::Compound(profile));
        Self::from_block_entity(Some(&NbtValue::Compound(root)))
    }
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
    #[test]
    fn item_wire_and_persistent_profiles_share_identity_and_patches() {
        use serde_json::json;
        let wire=Profile::from_item(&json!({"id":"00112233-4455-6677-8899-aabbccddeeff","name":"Head","properties":[{"name":"textures","value":"opaque","signature":null}],"body":"minecraft:entity/player/wide/alex"})).unwrap();
        let saved=Profile::from_item(&json!({"id":[1122867,1146447479,-2003195205,-857870593],"name":"Head","properties":[{"name":"textures","value":"opaque"}],"texture":"minecraft:entity/player/wide/alex"})).unwrap();
        assert_eq!(wire, saved);
        assert!(wire.present && wire.has_properties);
        assert_eq!(wire.textures.as_deref(), Some("opaque"));
        assert!(!Profile::from_item(&json!(null)).unwrap().present);
        assert_eq!(
            Profile::from_item(&json!("NameOnly"))
                .unwrap()
                .name
                .as_deref(),
            Some("NameOnly")
        );
        for bad in [
            json!({"id":"bad"}),
            json!({"id":[0]}),
            json!({"properties":[{"name":"textures"}]}),
            json!({"body":42}),
        ] {
            assert!(Profile::from_item(&bad).is_err());
        }
    }
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
