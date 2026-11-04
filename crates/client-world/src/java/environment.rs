//! Native dimension gameplay attributes.

impl super::WorldState {
    /// The pinned dimension attribute is authoritative; no dimension-name heuristic is used.
    pub fn fast_lava(&self) -> Result<Option<bool>, String> {
        let state = self;
        use java_protocol::nbt::Value;
        let bootstrap = state
            .bootstrap
            .as_ref()
            .ok_or("Missing native dimension configuration")?;
        let spawn = state.respawn.as_ref().unwrap_or(&bootstrap.world.spawn);
        let entry = bootstrap
            .configuration
            .registries
            .get("minecraft:dimension_type")
            .and_then(|r| {
                usize::try_from(spawn.dimension_type)
                    .ok()
                    .and_then(|i| r.get(i))
            })
            .ok_or("Missing native dimension type")?;
        let Value::Compound(fields) = &entry.value else {
            return Err("Invalid native dimension type".into());
        };
        let attributes = match fields.get("attributes") {
            None => return Ok(None),
            Some(Value::Compound(attributes)) => attributes,
            _ => return Err("Invalid native environment attributes".into()),
        };
        match attributes.get("minecraft:gameplay/fast_lava") {
            None => Ok(None),
            Some(Value::Byte(n)) if matches!(n, 0 | 1) => Ok(Some(*n != 0)),
            _ => Err("Invalid native fast-lava dimension attribute".into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use java_protocol::nbt::Value;
    #[test]
    fn lava_attribute_is_typed_and_follows_respawn_dimension_not_its_name() {
        let mut state = super::super::tests_support::state();
        assert_eq!(state.fast_lava().unwrap(), None);
        let entries = state
            .bootstrap
            .as_mut()
            .unwrap()
            .configuration
            .registries
            .get_mut("minecraft:dimension_type")
            .unwrap();
        let mut fast = entries[0].clone();
        fast.name = "fixture:dimension_with_arbitrary_name".into();
        let Value::Compound(fields) = &mut fast.value else {
            panic!()
        };
        fields.insert(
            "attributes".into(),
            Value::Compound([("minecraft:gameplay/fast_lava".into(), Value::Byte(1))].into()),
        );
        entries.push(fast);
        let mut spawn = state.bootstrap.as_ref().unwrap().world.spawn.clone();
        spawn.dimension_type = 1;
        state.respawn = Some(spawn);
        assert_eq!(state.fast_lava().unwrap(), Some(true));
        state.respawn.as_mut().unwrap().dimension_type = 2;
        assert!(state.fast_lava().is_err());
        state.respawn = None;
        assert_eq!(state.fast_lava().unwrap(), None);
    }
}
