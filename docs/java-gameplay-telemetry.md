# Native Java gameplay telemetry

Opt-in observations for the checklist in [STATUS.md](../STATUS.md). This is not a
server acceptance oracle. Counters may represent intentional cancellation, ordinary
corrections, rejected requests, or incomplete coverage. A passing unit test does not
qualify Hypixel, iOS, or every Check associated with that counter.

## Enable and collect

On desktop, set `CINNABAR_JAVA_TELEMETRY=1` before launching the usual build. It is off
when the variable and opt-in file are absent. No gameplay packet, physics constant,
prediction, or timeout is changed by enabling collection.

For an installed application, including iOS, place a file named
`java-telemetry.enabled` in the active profile's **logs directory**, containing only
`1` (an optional trailing newline is accepted). It is read once when a new Java
connection is created. Remove it and unset the environment variable to disable the
next connection. The iOS bundle enables Files document sharing; navigating to the
actual profile log directory still needs device verification. Do not put tokens or
account data in this file.

Disconnect normally after the test. The existing `java-join-latest.json` contains a
`telemetry` object only when enabled. Existing rotation retains five recent attempts.
The normal “Java join diagnostic saved” log identifies the active directory. This
is one summary per connection attempt, spanning its world epochs. Abrupt process/OS
termination before summary export is not guaranteed to save in-memory observations.
The developer-control state snapshot also exposes `native_java_telemetry` during a
run. Files and private captures belong outside version control.

A diagnostic iPhone build can explicitly enable telemetry without requiring access
outside the Files container:

```sh
CINNABAR_JAVA_TELEMETRY_BUILD=1 CINNABAR_IOS_PROFILE=play bash packaging/ios/build-ipa.sh
```

This compile-time opt-in is absent in ordinary builds. A runtime environment value
takes precedence over it; the flag file can independently opt in. Build without
`CINNABAR_JAVA_TELEMETRY_BUILD` to produce the normal off-by-default package. No new
IPA or physical-device verification is established by documenting this command.

## Meaning and limits

- Every counter/timing names its stable checklist IDs. Counters are independent
  observations: do not sum overlapping categories into a “bug total.”
- Correction magnitude is measured after the normal owner resolves relative flags,
  against the preceding local motion. Initial placement is counted separately. Only
  distances and queued input flags/context are retained, not world coordinates.
  Totals span the attempt's world epochs; the scope field is the last sampled scope,
  not a per-event replay timeline.
- Block requests, admitted sequence ACKs and authoritative updates are separate.
  ACK means the server processed a sequence, not that placement/mining succeeded.
  `coverage.local_block_prediction=false` means revert counters are **unobserved**.
  Lane C must connect `Prediction` hooks to real prediction/reconciliation before
  that coverage can become true. Telemetry does not implement prediction.
- Container refusal/staleness, queue backpressure, local write receipts, and refused
  authoritative updates are distinct. The protocol does not supply a universal
  “purchase succeeded” receipt. Compare actual authoritative item/currency state.
- Swing starts, the vanilla half-duration guard, explicit NONE gestures, missing
  descriptions, world-boundary clears, intentionally hidden hand frames, and missing
  hand publication are distinct. CPU publication does not prove GPU execution; inspect
  captured frames for visible behavior. A queued swing or published hand is not a hit.
- Use-stop context reports observed input/death/state conditions; “other” can mean
  normal completion or a server/item change and is not automatically a lost use.
- Entity collisions mean a spawn targeted an already resident ID. Missing-ID
  updates/removals use the current world owner; obsolete-world events are separately
  counted. Legitimate reuse after removal is not a collision.
- Frame intervals include between-frame waits; main-window wall time covers the
  FrameStart-to-Last observer interval, including worker reclaim. Neither is a GPU
  timestamp nor an exact CPU attribution. Terrain, entity/skin and hand-artwork
  scopes measure their actual main-thread publication functions, not the duration
  of asynchronous network downloads or GPU execution.
- The 50 ms spike threshold is a diagnostic threshold, **not a vanilla constant or
  performance acceptance budget**. Spikes include sampled pending chunk/skin/item
  loading flags for correlation; correlation does not establish the cause.
- Correction and timing-spike histories each retain at most 32 observations, so
  frame spikes cannot evict the correction context. Fixed-key counters/timings aggregate
  the whole attempt. No arbitrary server strings, chat, item components, account
  identities, credentials, skin URLs, or packet payloads enter the telemetry object.
- A timing with zero samples is unobserved. Functional tests under Xvfb/lavapipe
  cannot establish physical-iPhone or hardware frame-time performance.

Normal Java account/session, inventory, movement, and rendering owners remain the
only authorities. The passive handle survives the existing worker loan so frame
observations do not disappear while simulation/network processing continues.
