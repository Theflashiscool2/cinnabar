//! Bounded Java language resources; never consult a Bedrock numeric/item-component registry.
use super::source::{Error, Identifier};
use std::collections::BTreeMap;

const MAX_BYTES: usize = 4 * 1024 * 1024;
const MAX_ENTRIES: usize = 50_000;
const MAX_KEY: usize = 512;
const MAX_VALUE: usize = 16_384;

#[derive(Clone, Debug)]
pub struct Language {
    values: BTreeMap<String, String>,
    defaults: BTreeMap<String, serde_json::Value>,
}
impl Language {
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() > MAX_BYTES {
            return Err(Error::Budget("Java language bytes".into()));
        }
        let values: BTreeMap<String, String> = serde_json::from_slice(bytes)
            .map_err(|e| Error::Model(format!("Java language JSON: {e}")))?;
        if values.len() > MAX_ENTRIES
            || values
                .iter()
                .any(|(key, value)| key.len() > MAX_KEY || value.len() > MAX_VALUE)
        {
            return Err(Error::Budget("Java language entries".into()));
        }
        Ok(Self {
            values,
            defaults: BTreeMap::new(),
        })
    }
    pub fn get(&self, key: &str) -> Option<&str> {
        self.values.get(key).map(String::as_str)
    }
    /// Pinned native default ITEM_NAME components, not guesses from identifier prefixes.
    pub fn with_items<'a>(
        mut self,
        items: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> Result<Self, Error> {
        if !self.defaults.is_empty() {
            return Err(Error::Model("Java default item names already bound".into()));
        }
        let mut bytes = 0usize;
        for (identifier, component) in items {
            bytes = bytes
                .saturating_add(identifier.len())
                .saturating_add(component.len());
            if self.defaults.len() >= MAX_ENTRIES
                || identifier.len() > MAX_KEY
                || component.len() > MAX_VALUE
                || bytes > MAX_BYTES
            {
                return Err(Error::Budget("Java item names".into()));
            }
            Identifier::parse(identifier)?;
            let component = serde_json::from_str(component)
                .map_err(|e| Error::Model(format!("Java default item name: {e}")))?;
            if self.defaults.insert(identifier.into(), component).is_some() {
                return Err(Error::Model("duplicate Java default item name".into()));
            }
        }
        Ok(self)
    }
    pub fn item(&self, identifier: &str) -> Option<&serde_json::Value> {
        self.defaults.get(identifier)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_item_bindings_preserve_exact_default_components_without_key_guesses() {
        let lang =
            Language::load(br#"{"item.minecraft.stone":"Wrong","block.minecraft.stone":"Stone"}"#)
                .unwrap()
                .with_items([
                    (
                        "minecraft:stone",
                        r#"{"translate":"block.minecraft.stone"}"#,
                    ),
                    ("minecraft:air", "null"),
                ])
                .unwrap();
        assert_eq!(
            lang.item("minecraft:stone").unwrap()["translate"],
            "block.minecraft.stone"
        );
        assert_eq!(lang.get("block.minecraft.stone"), Some("Stone"));
        assert!(lang.item("minecraft:air").unwrap().is_null());
        assert!(
            lang.clone()
                .with_items([("minecraft:diamond", "null")])
                .is_err()
        );
        assert!(lang.item("minecraft:unknown").is_none());
        assert!(
            Language::load(b"{}")
                .unwrap()
                .with_items([("../stone", "null")])
                .is_err()
        );
        assert!(
            Language::load(b"{}")
                .unwrap()
                .with_items([("minecraft:stone", "null"), ("minecraft:stone", "null")])
                .is_err()
        );
    }
    #[test]
    fn invalid_or_excessive_resources_fail_boundedly() {
        for bytes in [b"[]".as_slice(), b"{\"name\":3}", b"null"] {
            assert!(Language::load(bytes).is_err());
        }
        assert!(Language::load(&vec![b' '; MAX_BYTES + 1]).is_err());
        let huge =
            serde_json::to_vec(&BTreeMap::from([("name", "x".repeat(MAX_VALUE + 1))])).unwrap();
        assert!(Language::load(&huge).is_err());
    }
}
