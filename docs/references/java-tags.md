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
session-scoped, not tied to a chunk or dimension epoch. Reconnect retires the old
bootstrap. Configuration retains tags when registries are reused; new registry
contents replace the previous sets. Retained tag allocations have
a 64 MiB safety budget, checked before any registry replacement is committed.

`java-protocol/tests/session/tags.rs` exercises compressed loopback transport,
updates around bootstrap, replacement/clearing, malformed data and subsequent
keepalives. `client-world/src/java/tags_tests.rs` exercises native admission and
session retirement. These are deterministic fixtures, not vanilla or Hypixel
live acceptance. Retaining current tag authority does not establish support for
every gameplay predicate that can consult tags.

Mining treats an omitted block tag registry like the native bootstrapped empty
sets, not a reason to suppress every destroy action. The verified runtime's
`BuiltInRegistries.freeze` calls `bindBootstrappedTagsToEmpty`; named holder-set
membership consults the block holder's bound tags. Tool defaults and literal
block rules still apply. Received tags replace these memberships; the client
does not invent vanilla tag definitions. The original `ProbeEmptyTags.java`
reference and empty-tag vectors cover this separately from the existing
server-data-tag vectors. Interaction tests cover omission, explicit clearing,
late updates, START/FINISH and cancellation without changing blocks locally.

Player collision restitution reads the runtime-exported `SUPPRESSES_BOUNCE` tag key
from the same current configuration. Slime support does not invent missing tag
memberships. Sneaking independently suppresses bounce and low-speed surface drag;
normal movement still uses the native block restitution and fractional contact.
The original `ProbeBlockCallbacks.java` and `ProbeSlime.java` references identify
the supported class and exercise the pinned runtime callbacks. Other special
movement callbacks and nonzero entity-bounciness attributes remain explicit gaps.
