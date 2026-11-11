# Native Java local capture/replay

A controlled **offline fixture** harness for the existing native client. No second world,
physics, inventory, authentication or rendering owner is introduced. Packet frames go
through the ordinary Java transport/codec, scoped admission, presentation and GPU pipeline.
These Python/Java files are test infrastructure, not shipped client runtime dependencies.

The target version/protocol, archive hashes and ordered packet registrations come from
`crates/java-protocol/data/{target.json,MinecraftCodec.java}`. Use the existing verified
server extraction/library cache and JDK named by that manifest. No automatic downloads,
plugin installations, replacement worlds, or borrowed OAuth identities are involved.

## Isolated server

Replace the four paths below with your existing verified inputs. Read the Minecraft EULA
before accepting it. `--root` must be a **new directory**; initialization refuses to overwrite
an existing world/config. Keep the root on a private local filesystem.

```sh
python3 tools/java-replay/harness.py init \
  --root /path/to/private/new-fixture \
  --server /path/to/server-26.3.jar \
  --libraries /path/to/existing-server/libraries \
  --java-home /path/to/jdk-25 \
  --accept-eula
python3 tools/java-replay/harness.py run --root /path/to/private/new-fixture
```

`run` verifies the official bundler, extracted server and every library against the bundler
manifests, compiles the authored acceptance helpers, then starts vanilla. The Java executable
must match the pinned server's requirements. RCON credentials stay in a mode-0600 private
config/properties; they are not printed. Game/RCON listeners bind numeric loopback. Default
ports are 25586/25585. Native floating checks and normal authoritative transactions remain on.

In another terminal, once the server reports ready:

```sh
python3 tools/java-replay/harness.py setup --root /path/to/private/new-fixture
```

This constructs a small platform at 96..127, ground Y=68, with slabs, stairs, fences,
trapdoors, tight corners, honey, cobweb, slime and water. A chest contains wool/tools, fire
charges, TNT, pearls, bow/arrows and a potion. An invisible armor stand supplies a hologram.
`acceptance_shop` creates a real villager whose native main-hand interaction opens a
chest-style server menu. Its center cell sells four wool for one iron. The server alone
checks stock/currency/capacity and mutates inventory. Player-area clicks use native vanilla
handling; shop cells cannot be taken as ordinary storage. This is an **authored fixture**,
not a Hypixel shop or generic merchant implementation. Recreate it with `acceptance_shop`
after server restart; persistence does not preserve the Java test subclass.

There is no Bed Wars plugin in this fallback. Vanilla fire charges do not acquire Bed Wars
fireball behavior, and placing a chest of utility items does not establish utility/combat
acceptance. Movement, mining math and utility checks belong to their owning lanes.

The one timeboxed Paper/BedWars1058 metadata pass returned HTTP403 for both endpoints.
Compatible pinned builds could not be established; this is not evidence none exist.
Do not repeatedly probe the same blocked endpoints.

## Graphical scenario

Launch the existing desktop diagnostic client at 1024x576 with developer-control enabled,
`CINNABAR_JAVA_OFFLINE_FIXTURE=CinnabarTest` and `CINNABAR_JAVA_TELEMETRY=1`. Use its normal
verified assets and graphical backend. Start at the menu; the runner refuses to retire an
existing user session. No normal account credentials are needed or recorded.

```sh
python3 tools/java-replay/run_fixture.py \
  --endpoint /path/to/client-endpoint.json \
  --fixture /path/to/private/new-fixture/fixture-private.json \
  --out /path/to/private/new-evidence-directory
```

The runner connects, waits for spawn rendering, supplies fixture currency/items through
server administration, then uses **real main-hand and pointer input** to open/buy/close.
RCON independently verifies iron 8→7 and wool 16→20. It requests a loopback transfer,
waits for the full configured stationary view and destination HUD/inventory, disconnects,
then replays both captured connections into the same native client and repeats those inputs.
Inspect the live/replay shop and destination screenshots. Do not call process creation or
an uninspected screenshot graphical acceptance.

Outputs include a checksummed transcript, state projections/comparison, telemetry differences,
server receipts, screenshots and tested helper-source hashes. `comparison.json` fails on wrong
inventory/HUD/world counts or increases in named functional anomalies. Disabled/missing
telemetry or an incompatible telemetry schema fails comparison rather than passing silently.
The transcript includes packet/profile data for the **synthetic fixture**; keep the entire
output directory private. Never commit captures, account data, logs or game assets.

## Standalone capture, replay and assertions

```sh
python3 tools/java-replay/session.py capture --tape /private/new-session.jsonl \
  --target 127.0.0.1:25586 --listen 127.0.0.1:25587 --sessions 2
python3 tools/java-replay/session.py inspect --tape /private/new-session.jsonl
python3 tools/java-replay/session.py replay --tape /private/new-session.jsonl \
  --listen 127.0.0.1:25587 --speed 1
```

Only the exact named offline profile/UUID is admitted. Online encryption, real profiles,
non-loopback destinations/transfers, incompatible versions, malformed framing, bad checksums,
incomplete recordings and oversized tapes are rejected. The recording is mode0600 and
exclusive-create; interrupted recordings have no completion footer and cannot be replayed.
The fixture budget is 256MiB, 100000 records and one hour, with bounded decompression. It
is a testing budget, not a change to the production client's protocol limits.

Capture forwards original wire bytes unchanged except a loopback transfer's destination,
which is normalized to the owned listener. Authentication/login is not recorded except the
validated offline login-finish facts needed to establish the same synthetic identity.
Configuration/play packet payloads are recorded after decompression in original order.
Replay uses uncompressed framing, synthetic local offline login and the recorded packets;
all decoding/admission remains production code. Native configuration/teleport/keepalive
replies and recorded gameplay/menu requests are gated by phase and packet body. Perform
the original normal input actions at those gates; otherwise replay fails with the missing
request. No client requests are fabricated by the replay server. Multiple connections and
server-directed transfers are supported; external hosts are never followed by this fixture.
Replay holds final state until explicit disconnect, sending normal fixture-server keepalives
at the pinned vanilla 15-second interval and requiring real native replies. This avoids
inspection-time idle disconnects without changing client timeouts. Pending gated replies
are bounded by count and bytes; neither heartbeat nor overflow fabricates a client reply.

`check.py` takes an explicit expectation list (`path`, `op: eq|min|contains`, `value`) and
reads ordinary developer state without copying its endpoint token to results:

```sh
python3 tools/java-replay/check.py --endpoint /private/client-endpoint.json \
  --expect /private/expectations.json --out /private/assertions.json
python3 tools/java-replay/compare.py /private/live-state.json /private/replay-state.json \
  --out /private/comparison.json
python3 -W error::ResourceWarning -m unittest discover -s tools/java-replay -v
```

An example expectation is `[{"path":"/native_java/phase","value":"Play"}]`. Missing paths
fail. The default comparison covers phase, stationary column/entity counts, full inventory
storage, selected slot/open menu, sidebar, health/food and inventory errors. It does **not**
assert every block in the world, pixel identity, skin downloads, animation timing, or frame
performance. Functional counters are compared; timing counters are retained as observations,
not deterministic equality assertions. Expected non-anomaly differences are reported explicitly.

Only the live run's server receipt proves that run's purchase. Replay verifies native input,
packet generation and state admission against a captured response; it is not a new authoritative
purchase, damage or physics result. Neither establishes authenticated online, Hypixel, iOS
simulator, physical-iPhone or complete Bed Wars acceptance. Extend scenarios with owning-lane
fixtures and separate live authority checks before promoting those checklist items.
