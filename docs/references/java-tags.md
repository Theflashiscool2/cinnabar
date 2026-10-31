# Java registry tag updates

Scope: Java 26.3, protocol 777, as pinned in
`crates/java-protocol/data/target.json`.

The pinned [MCProtocolLib packet definition](https://github.com/GeyserMC/MCProtocolLib/blob/b4fa8b23064bb85d7c7966cc2cd6bf369c9c1405/protocol/src/main/java/org/geysermc/mcprotocollib/protocol/packet/common/clientbound/ClientboundUpdateTagsPacket.java)
defines a registry-keyed map of tag names to registry entry IDs. The checked-in
`MinecraftCodec.java` registers it in both configuration (14) and play (137).
It has no response packet. This is registry metadata, not a request to restart
configuration or replace a world.

The connection uses the same decoder in both phases and validates the entire
packet before admission. Updates before world login enter the bootstrap. Later
updates use the ordered, generation-stamped session channel. The world owner
replaces each supplied registry's complete tag set; omitted registries remain,
an empty registry clears its tags, and an empty packet is a no-op. Tags are
session-scoped, not tied to a chunk or dimension epoch. Reconnect/configuration
retirement discards the old bootstrap and its tags. Retained tag allocations have
a 64 MiB safety budget, checked before any registry replacement is committed.

`java-protocol/tests/session/tags.rs` exercises compressed loopback transport,
updates around bootstrap, replacement/clearing, malformed data and subsequent
keepalives. `client-world/src/java/tags_tests.rs` exercises native admission and
session retirement. These are deterministic fixtures, not vanilla or Hypixel
live acceptance. Retaining current tag authority does not establish support for
every gameplay predicate that can consult tags; mid-session reconfiguration
remains explicitly unsupported.
