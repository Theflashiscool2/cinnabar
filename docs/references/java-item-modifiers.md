# Java item attribute modifiers

Scope: Java 26.3/protocol 777 from `crates/java-protocol/data/target.json`.
Wire reference: [ItemTypes.readItemAttributeModifiers](https://github.com/GeyserMC/MCProtocolLib/blob/b4fa8b23064bb85d7c7966cc2cd6bf369c9c1405/protocol/src/main/java/org/geysermc/mcprotocollib/protocol/data/game/item/component/ItemTypes.java)
at the same pinned MCProtocolLib revision. The checked-in, hash-verified
`ItemAttributeModifiers.java` and `ModifierOperation.java` supply generated enum
tables; the existing attribute registry supplies attribute identities.

Each list entry contains an attribute VarInt, modifier resource identifier,
double amount, operation VarInt, equipment-slot-group VarInt, and display VarInt.
Only the override display adds an anonymous NBT text component. There is no
trailing show-tooltip boolean in this version. Unknown display IDs use the
reference codec's default display; invalid attributes, operations, slots,
identifiers, nonfinite amounts and truncated data fail before item publication.

The item component patch retains the ordered modifier list, amounts, slot rules
and display text in the existing inventory model. An empty override and removal
of the component remain distinct. Receiving an item does not apply its bonuses
to the player: server entity-attribute updates remain the existing authority.
Full modifier tooltip presentation and combat acceptance are separate work.

Fixtures cover all three operations, eleven slot groups and three display modes,
unknown-display fallback, following-component alignment, malformed/truncated
items, compressed/encrypted session transport, native inventory retention,
replacement and stale-session rejection. These are automated fixtures, not a
claim of Hypixel lobby or iPhone gameplay acceptance.
