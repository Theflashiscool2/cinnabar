# D entity acceptance (source candidate; not executed)

Use the existing isolated `harness.py` world and verified 26.3 server. Do **not** rerun
`setup`, reset that world, or connect to a public server. Root must first pass the D Rust
checks and build e44cd80df or its reviewed descendant. This procedure does not validate
fluids, armor trim, arbitrary item use, or a public server implementation.

Two real native client instances are required: viewer `CinnabarTest` and peer `DPeer`,
with distinct developer-control endpoints and
`CINNABAR_JAVA_OFFLINE_FIXTURE=CinnabarTest` / `DPeer`. Enable
`CINNABAR_JAVA_TELEMETRY=1` in both. Use the same verified product binary/resources;
set separate endpoint files/window state using the existing launcher. The server already
allows two players. Set `CINNABAR_DEVELOPER_CONTROL` to a different private endpoint file for each.
Connect the peer directly to its loopback game port; only the viewer
goes through the existing capture proxy. `session.py` intentionally accepts only the
pinned `CinnabarTest` identity: do not change that policy to proxy the peer.

The optional console command `acceptance_d <viewer> <peer> <phase>` is registered beside
the existing shop fixture at harness startup. It requires two distinct connected real
players in the overworld. It changes native equipment/use/crouch state; it does not
construct players or send invented entity/collect metadata. Its actions are **server
fixture actions**, not accepted bow attack/input tests. Prefer ordinary peer developer
input for the actual bow/sneak/release/swing pass; use the command pass to isolate remote
presentation if ordinary input fails. A failure of ordinary input remains a separate gap.

The commands below assume shell variables `FIXTURE` (existing private config), `VIEW`
and `PEER` (private endpoint files), and a new private `OUT` directory. Run from the repo
root. Endpoint/config contents contain secrets: preserve mode 600, never publish them.

```sh
mkdir -m 700 "$OUT"
python3 tools/java-replay/session.py capture --tape "$OUT/session.jsonl" \
  --listen 127.0.0.1:25587 --target 127.0.0.1:25586 --sessions 2
```

Leave the proxy running in its own terminal. Reuse the existing authenticated RPC helper:

```sh
dev() { python3 -c 'import sys,json; sys.path.insert(0,"tools/java-replay"); from check import rpc; print(json.dumps(rpc(sys.argv[1],json.loads(sys.argv[2]))))' "$1" "$2"; }
dev "$VIEW" '{"cmd":"connect_java_fixture","address":"127.0.0.1:25587"}'
dev "$PEER" '{"cmd":"connect_java_fixture","address":"127.0.0.1:25586"}'
```

Use the fixture's actual loopback game port if different from 25586. RCON below uses
`harness.py command`:

```sh
python3 tools/java-replay/harness.py command --root "$(dirname "$FIXTURE")" --command 'tp CinnabarTest 100.5 69 106.5 0 0'
python3 tools/java-replay/harness.py command --root "$(dirname "$FIXTURE")" --command 'tp DPeer 100.5 69 109 180 0'
python3 tools/java-replay/harness.py command --root "$(dirname "$FIXTURE")" --command 'clear CinnabarTest minecraft:iron_ingot'
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase equip --out "$OUT/01-equip"
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase bow --out "$OUT/02-bow-fixture"
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase sneak --out "$OUT/03-sneak-fixture"
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase stop --out "$OUT/04-release-fixture"
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase block --out "$OUT/05-block-fixture"
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase stop --out "$OUT/06-stop"
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase drops --out "$OUT/07-settled"
```

Inspect each PNG and the exact client snapshots, not just process exit. Confirm worn
head attachment and chest/held equipment, bow arm/body alignment and pulling model,
crouch, and immediate release/block transitions without a retained charged mesh. Stop
and repeat normal peer input separately: peer `input` MouseRight held (bow in main hand),
then `release_all`; Shift for crouch, MouseLeft for swing. Supply arrows on the real
server first (`give DPeer minecraft:arrow 16`); a bow without ammunition cannot establish
ordinary bow input. For example:

```sh
dev "$PEER" '{"cmd":"input","press":["MouseRight"]}'
# While held, run the viewer snapshot checkpoint, then release:
dev "$PEER" '{"cmd":"input","release_all":true}'
```

Capture viewer `snapshot`
checkpoints during each. Do not claim an `equip` screenshot validates player-use poses.

The head uses the **actual peer GameProfile** through native `ResolvableProfile`.
An offline peer normally has no texture property: this proves default head geometry,
not a resolved custom texture. Custom texture acceptance additionally requires an
existing trusted, verified profile/skin fixture with known expected pixels, supplied
through the native profile component. No random URL, fabricated texture property, or
network skin-service success is assumed here. Keep this limitation explicit.

`drops` creates two ordinary 3-iron native ItemEntities above the existing slab at
100/110 and stair at 102/110, zero initial velocity, ordinary gravity, delayed pickup.
It waits for authoritative `on_ground` and count=3 for both. Check server positions
against the native client's matching network IDs and `item_motion` snapshots, including
`active`, `fallback`, on-ground/support position and retained interpolation. Compare
against the server's *actual* stair contact surface, not a guessed top height. A client
sample is capped at 16 entities; if an item ID is absent, this is insufficient evidence,
not proof that it vanished. Do not add unrelated entities before this pass.

```sh
python3 tools/java-replay/harness.py command --root "$(dirname "$FIXTURE")" --command 'tp CinnabarTest 100.5 69.5 110.5 0 0'
python3 tools/java-replay/entity_checkpoint.py --endpoint "$VIEW" --fixture "$FIXTURE" --peer DPeer --phase pickup --out "$OUT/08-pickup"
```

The command only removes native pickup delay and sets the collector UUID; native
`ItemEntity.playerTouch` decides collection. The helper requires exactly +3 authoritative
iron, leaving the distant stair stack untouched. It preserves before/after server item
counts and 2 seconds of successive native client states. Look for
`native_java_entities.published_pickups > 0` and the item's authoritative retirement;
the flight is only three logical ticks. Polling after synchronous RCON can miss it:
absence of a captured transient is **inconclusive**, not visual acceptance. For a
reliable graphical pickup judgement record the desktop before issuing the command.
Do not infer inventory awards from rendered pickup particles.

Disconnect only the viewer with `dev "$VIEW" '{"cmd":"disconnect"}'`; wait until its
`native_java` is null. Reconnect it through the same proxy (second captured session).
Keep the peer and remaining ordinary item alive. Save a `snapshot` checkpoint under
`09-reconnect`: inspect epoch retirement, restored peer/equipment/head, remaining
count=3 on the server, no ghost collected item, and no retained pickup or charged mesh.
Disconnect viewer again and wait for the two-session proxy to exit successfully.

Replay uses the same real native client and captured inbound stream:

```sh
python3 tools/java-replay/session.py inspect --tape "$OUT/session.jsonl"
python3 tools/java-replay/session.py replay --tape "$OUT/session.jsonl" --listen 127.0.0.1:25587 --speed 1
```

Connect viewer through replay twice as above. Do **not** issue fixture/RCON actions
while replaying. Use `entity_checkpoint.py --replay --endpoint "$VIEW" --peer DPeer
--phase snapshot --out "$OUT/replay-<checkpoint>"` to retain snapshots and screenshots
at observed corresponding phases; normal camera input may be repeated. Retain proxy
logs and packet timestamps. `--replay` never talks to the server. Reconnect is a new
session, not a transfer packet. Check both replay segments completed successfully.

Replay currently has no D semantic pause/checkpoint barrier; manual observation at
speed 1 is required for intermediate phases. Timing-dependent animation ticks and
async pending/resource counts must not be asserted equal across arbitrary frames.
Record exact snapshots and compare stable network identities, static settled item
positions/counts (server receipts for live; packets for replay), inventory totals and
retirement. Inspect corresponding screenshots for pose/equipment correctness. Existing
`compare.py` shop baselines alone do not establish these D properties. Replay proves
native inbound admission/presentation, not fresh authoritative collection or peer input.

Finally `acceptance_d CinnabarTest DPeer cleanup` removes only tagged D items and stops
peer use/crouch. Equipment changes intentionally remain on this controlled peer; record
its prior inventory if restoration is desired. The fixture never clears unrelated
entities, rewrites blocks, or resets the existing world. Keep all evidence private and
report unexecuted/missed graphical and texture cases explicitly.
