# Native Java session integration

Target remains Java26.3/protocol777. This is a subsystem adaptation into Cinnabar,
not an embedded second client or a claim of iPhone/Hypixel gameplay qualification.

## Inspected sources (2026-11-04)

| Source revision | License actually inspected | Integration decision |
| --- | --- | --- |
| `horizon-rust/horizon` `59d8cc38a2627f5b7aa0c96fc91b013df70c45fc` | Root MIT, copyright2026 Ludovic Marie and contributors | Version26.3/protocol777 matches. Adapt `horizon-client/src/net.rs` network/game-thread dispatch and configuration handoff into the existing Tokio session. Its complete client brings another world, player, renderer, event loop and resource graph; that is not a drop-in Cinnabar dependency. |
| `Vasysik/horizon-android` `6eca278fd92e99ade83ae5823d090cb4bdc6d046` | Root MIT checked independently | Existing older reference ports retained with their original provenance. Android application/platform integration is not evidence of iOS compatibility; no Android platform code imported in this change. |
| `azalea-rs/azalea` `0d38e0dcf47eb258c41b1b09ebc2bcde7253e185` | `LICENSE.md`, MIT, copyright2022 mat | Client package targets26.2, requires nightly feature `error_generic_member_access`, starts its own ECS runner and does not supply a renderer. No code imported; adopting its player/world wholesale would create competing owners and a version migration. |

Direct source inspection included manifests, version constants, network dispatch,
remote player admission and licenses. READMEs and footage were not acceptance.
The MIT notice is shipped in `assets/licenses/Native-Java-Reference-MIT.txt`.
No JVM, JIT, additional runtime client or new framework dependency was introduced.

## Single authority contract

| State | Owner | Adapter boundary |
| --- | --- | --- |
| Connection, framing, encryption, protocol state | `java-protocol/session` | One Tokio connection actor, cancellation-safe framed stream; bounded FIFO ingress |
| World/chunks/registries | `client-world/java` + world domain | Generation/epoch-admitted events; existing extraction/publication |
| Player simulation/corrections | `sim/java`, `gameplay/java`, `client-world/java/player` | Pose receipts resolve relative fields before protocol acknowledgements |
| Entities | Native entity lifetime/admission domain | Existing generation/epoch/network-ID incarnations |
| Inventory/revisions | `inventory/java`, `client-world/java/inventory` | Existing server-authoritative model, scoped transaction receipts |
| Input/action order | Existing semantic controls, loaned native movement controller and session command channel | Same20Hz tick owner on either the frame or service, never both; one ordered action channel |
| Accounts/credentials | `launcher/java_accounts`, launcher host, platform credential backend | Selected Java account only; iOS Keychain, no implicit offline fallback |

## Active integration slice

Previously, awaiting an admitted teleport stopped reading every play packet and
aborted after ten seconds. Rendering latency could therefore suppress keep-alives.
The repaired actor services network keep-alives while retaining ordered game frames
in a bounded FIFO. Capacity applies backpressure rather than dropping packets.
A full game-event channel uses the same bounded, codec-boundary-aware read-ahead:
network KeepAlive replies remain live, deferred facts retain FIFO order, and a real
disconnect reason is surfaced. The memory watermark counts retained payload capacity.

The existing Java session, world and movement controller are lent exclusively to a
between-frame worker and reclaimed before `First`. It admits the same events and
corrections and runs the same20Hz simulation while graphics blocks. A shared elapsed
clock prevents double-counting that work on the next frame. `PlayerLoaded`, world
scope, collision availability and correction authority remain required; no parallel
world, inventory or physics engine is introduced.

Pings are game-thread barriers: Pong follows domain admission and cannot overtake
an earlier correction. Teleport IDs may be reused, but receipts still require the
matching sequence and dimension epoch. Transport idle timeout and cancellation remain.

Reconfiguration reuses the authenticated encrypted stream and cookies, retires old
play authority and publishes Configuration before the replacement bootstrap. New
registry contents replace decoding context; a registry-free or tag-only round trip
retains the last server-provided registry set, matching the pinned vanilla
RegistryDataCollector rather than inventing defaults; old input cannot serialize before new login.
Configuration uses the existing login timeout, without a new unbounded wait.

Acceptance and exact current evidence are maintained in the recovery checkpoint;
structural/fixture tests are not vanilla gameplay, iOS or online acceptance.

## Reproducing a real configuration handoff

`docs/testing/fixtures/VanillaDebugServer.java` only enables the pinned vanilla
`debugconfig` administrative command, normally gated by `IS_RUNNING_IN_IDE`.
It calls the official server entry point. It does not synthesize server responses,
change player authority, patch the JAR or run an alternative world implementation.
Use an isolated offline loopback acceptance world, accepted EULA, JDK25 and the
already integrity-verified26.3 server artifact with its extracted library classpath:

```sh
javac -cp "$VANILLA_SERVER_CLASSPATH" -d "$ACCEPTANCE_DIR/classes" \
  docs/testing/fixtures/VanillaDebugServer.java \
  docs/testing/fixtures/VanillaMovementTrace.java
# In the disposable server directory containing server.properties/eula.txt:
java -Xms256M -Xmx768M -cp "$ACCEPTANCE_DIR/classes:$VANILLA_SERVER_CLASSPATH" \
  VanillaDebugServer nogui
```

Connect the real Cinnabar native Java client. In server console run
`debugconfig config <player>`, observe world/menu/input retirement, then within
30seconds run `debugconfig unconfig <player-uuid>`. This exercises the official
configuration/play transition on the same connection, including a round trip
without new registry contents. Inspect the restored world and verify a normal
client action against server state. Repeat, then disconnect/reconnect. Label this
as a **vanilla development-command fixture**, separately from production Hypixel
backend acceptance. Never enable this debug bootstrap on a public server.

The wire regression separately sends replacement registries, retained registries,
play tag updates, reused teleport IDs and stale input, both encrypted and plain.
The domain regression verifies biome IDs/colors, world epoch, pending receipts,
vitals and stale-session cleanup. These are fixture evidence, not screenshots.

## Entity-use integration

The same connection and action owners now carry main-hand entity use for already
presented players and pigs. `ServerboundInteractPacket` in26.3 includes entity ID,
hand, low-precision Vec3 hit location and secondary action; it has no legacy action
opcode. LP packing is adapted from the same pinned upstream `horizon-protocol/src/buf.rs`
under the shipped MIT notice. Independent vectors come from the official pinned
server codec (the original generator and bytes are in `java-protocol/tests/fixtures`).

The picker retains the actual entry point relative to the current interpolated
entity position, including baby dimensions, block occlusion and negative boundaries.
The existing action FIFO retains that point/rotation/sneak through hotbar backpressure.
The connection rejects stale spawn tokens, world epochs and correction sequences.
Only server updates change items, health or menus. New inputs must not be dropped
merely because an incoming game-thread Ping awaits its Pong receipt: Ping changes
neither pose authority nor the order of already queued outgoing player actions.
An independent framed-wire regression reproduces the old drop and verifies the fix.

Only ordinary supported instant item fallback is integrated. Active use, complete
offhand/result prediction, unsupported entity presentation and specialized menus
remain separate gaps; a successful pig interaction is not NPC-shop acceptance.

### Player-list removal and world entities

The pinned client's `AbstractClientPlayer.getPlayerInfo()` caches its first resolved
PlayerInfo. Removing a tab entry does not despawn that entity or change its cached
game mode. Native admission retains the removed mode for that live entity identity;
a new profile with the same UUID cannot change an older entity's cached information.
Entity removal/replacement, world replacement, configuration and disconnect retire
the retained facts. Picking excludes known spectators, rather than treating every
missing tab entry as a spectator. No per-frame roster scan is added.

The acceptance-only `VanillaDebugServer` fixture optionally registers console-only
`acceptance_unlist <target> <viewer>` and `acceptance_relist <target> <viewer>`.
Start the same verified server with `--add-opens java.base/java.lang=ALL-UNNAMED`
and `-Dcinnabar.acceptance.entityCommands=true`. Both names must identify real
connected players. The command sends the official native player-list packet to
the viewer; entity position, health and interactions remain owned by the server.
This instrumented offline fixture tests tab-list/entity independence, not a
production NPC-shop plugin, online authentication or Hypixel compatibility.

Pig fallback eligibility comes from the version-pinned offline item-facts exporter,
including actual spawn-egg entity defaults; the host does not classify native items
by name suffix. The extended schema is integrity checked during protocol generation.
Linux Xvfb/lavapipe official vanilla offline acceptance exercised adult/baby feeding,
sneak hit points, negative chunk seams, removal/replacement, unload/revisit, reconnect
and a server-authoritative damage change. The final facts-integrated executable was
also inspected live: a normal right-click changed the server carrot count9→8 and
baby age, with the updated count visible in the real HUD. This is not NPC-menu,
online-mode, iOS or Hypixel acceptance.

Actual two-native-client Linux/lavapipe offline execution confirmed roster2→1 while
one player stayed visible and selected, server damage19→18, landing, and observer
reconnect followed by another unlisted-player hit18→17. The initial simultaneous
cold joins also produced a30s capture timeout and one floating kick after damage;
that unresolved cold-load observation is retained, not counted as passed. Warm
repeats passed without changing server flight policy. Physical-iPhone, online-mode
and Hypixel NPC-menu acceptance remain untested.

For bounded movement diagnostics, also set `-Dcinnabar.acceptance.trace=<private-directory>`
with `entityCommands`. Console `acceptance_trace <player>` samples actual server
position, velocity, collision, load/teleport state and native floating counters.
`acceptance_keepalive <player>` makes one native probe due early, preserving its
actual response validation and timeout; this is instrumented evidence, not a server
policy or client fix. Neither command changes player authority or flight checks.
