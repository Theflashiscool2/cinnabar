# Native Java item-component coverage

Scope is the release/protocol in `crates/java-protocol/data/target.json`:
Java 26.3/protocol 777. Rust readers preserve all 122 registered component types
in the existing authoritative inventory patch. Omitted defaults, removed
components and explicitly supplied values remain distinct. Resource keys,
profile properties, URLs and server text remain inert data; receipt does not
fetch skins, execute text, apply equipment bonuses or consume an item.

Wire field order comes from the pinned MCProtocolLib revision
`b4fa8b23064bb85d7c7966cc2cd6bf369c9c1405`:

- [ItemTypes.java](https://github.com/GeyserMC/MCProtocolLib/blob/b4fa8b23064bb85d7c7966cc2cd6bf369c9c1405/protocol/src/main/java/org/geysermc/mcprotocollib/protocol/data/game/item/component/ItemTypes.java)
- [MinecraftTypes.java](https://github.com/GeyserMC/MCProtocolLib/blob/b4fa8b23064bb85d7c7966cc2cd6bf369c9c1405/protocol/src/main/java/org/geysermc/mcprotocollib/protocol/codec/MinecraftTypes.java)

These are behavior references, not a second runtime client. Registry identity and
reader selection come from the checked, generated component table. The implementation
is original Rust. Important distinctions include templates' ID-before-count order,
inline versus registered holders, dynamic versus static profiles, filterable text,
and recursive hidden effects/component matchers. Unknown unions and malformed
payloads fail before inventory publication, with the enclosing play packet named.
No guessed opaque-byte skipping is used.

The synthetic corpus has an independently assembled payload for each component;
it does not use a production encoder. Tests compare its name set with the generated
registry, parse a following sentinel component, reject every truncated prefix and
trailing bytes, and assert representative field values. Additional vectors cover
dynamic profiles, all consume-effect alternatives, hidden effects and exact/partial
matchers. The encrypted/compressed transport test delivers the whole corpus in
ordered inventory snapshots, then checks the server receives a keepalive response.

```sh
cargo test --locked -p java-protocol --test item_corpus
cargo test --locked -p java-protocol --test session -- --nocapture
```

Limits include existing wire/NBT budgets, 64 nested component/effect levels and a
conservative expanded JSON budget of 8 MiB per patch/list. These are explicit
provisional client admission limits, not proof of vanilla parity for arbitrarily
large custom data. The corpus is representative of each type, not every possible
value, conditional branch, tooltip, visual or gameplay behavior. Entity metadata
has additional independent schemas. Real server gameplay acceptance remains open.

## Remaining packet gates

`census_all_play_packet_dispatch_gates` tests every generated clientbound play
identity in an isolated connection so one failure cannot hide the next. Its empty
bodies distinguish explicit unsupported gates from recognized handlers rejecting
missing data and empty/optional notifications. Passing this census does **not**
mean those gates work, optional observations are rendered, or required packet
fields have exhaustive coverage.

| Explicit gate | Required implementation |
| --- | --- |
| `ClientboundMountScreenOpenPacket` | Mount inventory layout and lifecycle |
| `ClientboundMerchantOffersPacket` | Authoritative offers and merchant UI |
| `ClientboundMoveVehiclePacket` | Vehicle corrections and movement |
| `ClientboundOpenBookPacket` | Book presentation/input |
| `ClientboundOpenSignEditorPacket` | Native Java editor wire/UI lifecycle |
| `ClientboundPongResponsePacket` | Play latency-response handling |
| `ClientboundPlaceGhostRecipePacket` | Recipe preview/slot-display schema |
| `ClientboundPlayerChatPacket` | Signed player-message and acknowledgment state |
| `ClientboundStartConfigurationPacket` | Configuration transition, world teardown and fresh bootstrap |
| `ClientboundTestInstanceBlockStatus` | Developer test-block response schema |
| `ClientboundTransferPacket` | Transfer destination/session policy and handoff |
| `ClientboundShowDialogGamePacket` | Interactive server dialogs |

Conditional gates include required server packs, configuration consent/dialogs and
experimental features. The test checks the explicit set so a schema/dispatch change
forces review. It deliberately retains required gates instead of silently calling
unsupported behavior successful. Signed chat, reconfiguration and server-menu
behavior are likely priorities for network-server play, not confirmed observations
of a particular Hypixel session.
