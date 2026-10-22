# Native Java connection foundation

This crate owns Java wire framing, discovery, authentication and the ordered
login/configuration/play connection. It uses native TCP, independently of the
Bedrock Go core. It does not implement a rendered or playable Java world.

The exact supported release and protocol, official server checksum, and pinned
protocol-data revisions live in [data/target.json](data/target.json).
`build.rs` derives state-specific packet tables from the pinned, MIT-licensed
`data/MinecraftCodec.java` reference and checks its version against the manifest.
That file is build input only, never compiled or run as a Java client.
`Cargo.lock` pins Rust dependencies. Other Java versions are rejected by the
launcher and acceptance tool; a status response alone never establishes support.

## Integration and exercise

Saved servers without `edition` remain Bedrock. In Add Server, select Java
Edition. An empty port means Java's default and permits SRV discovery; an
explicit port suppresses SRV. The original hostname remains the handshake
identity while DNS/SRV selects the socket destination. Java discovery runs on a
cancellable worker; description text is bounded plain text, never executable.
Favicons are retained only as bounded status data and never fetched or rendered.
A valid status survives a missing pong with unknown latency. A malformed or
mismatched pong is an error. Offline, timeout and malformed replies stay distinct.

The app routes Java joins to `client-session::java::Session`, which owns a
bounded event FIFO and abortable task. Retirement cancels preparation and socket
work; `client-world::java::WorldState` rejects old generations before mutation.
The launcher shows the actual protocol state and bootstrap facts, explicitly
stating that world rendering is unavailable. Cancel, retry and server switching
use the existing session lifecycle. Java credentials have their own namespace;
Bedrock credentials and transports are never reused as Java identities.

Build the developer acceptance path:

```sh
cargo build -p java-connect
cargo test -p java-protocol
cargo run -p java-connect -- --servers /path/to/servers.json --index 0 \
  --accounts /path/to/java-account-directory --keepalives 2 --reconnect 2
```

The saved-server file is the existing launcher JSON array, for example:

```json
[{"name":"Local Java fixture","address":"127.0.0.1:25567","edition":"java"}]
```

For an explicitly authorized, local **offline-mode test fixture only**, replace
`--accounts ...` with `--offline-fixture NativeFixture`. Every resolved destination
must be loopback. The tool requires bootstrap, position and actual received and
answered keepalives in each generation; a timer is only a failure deadline.
It reports typed JSON facts, disconnects, and repeats with a new generation.
It never downloads a server or accepts its EULA. Obtain the official server
identified by the manifest, verify its checksum, accept the EULA yourself, and
run it with the manifest's required Java runtime. Bind a fixture to loopback;
use a whitelist and add the fixture identity. Do not change a production server's
online-mode setting. Server binaries, assets and raw captures stay outside Git.

`java-status HOST [PORT] [PROTOCOL]` remains a low-level status probe with strict
pong semantics; the launcher uses the more tolerant `transport::discover` API.

## Authentication

Register this fork's own Microsoft public-client application, enable the device
code flow for personal accounts, and obtain Minecraft service approval. Device
flow has no redirect URI. No application is supplied or borrowed by this fork.
The user's environment currently has no application, so real online-mode
acceptance remains blocked.

```sh
cargo run -p java-connect -- account-login /path/to/java-account-directory PUBLIC_CLIENT_ID
cargo run -p java-connect -- account-select /path/to/java-account-directory PROFILE_UUID
cargo run -p java-connect -- account-logout /path/to/java-account-directory PROFILE_UUID
```

For the graphical client, use the `java` sibling directory of its Bedrock auth
cache. Sign-in writes public `oauth-client.json` there and stores tokens with the
existing protected, atomic account-file helpers. Unix files are mode 0600.
The flow handles device polling, decline, expiry, cancellation, refresh, Xbox user
and Java XSTS exchange, Java entitlement/profile checks, and server session join.
An active graphical or diagnostic session closes when its selected Java account
is switched or logged out; the account monitor checks once per second. No credentials or service response bodies are
included in errors or diagnostic output. Device prompts necessarily display the
short-lived user code; do not publish that output.

The official device-flow contract was checked against Microsoft's
[documentation](https://learn.microsoft.com/en-us/entra/identity-platform/v2-oauth2-device-code),
revision `a4be4ac419c4e857b1c4de7dee22c9f7e0c750f9`, on 2026-10-22.
Minecraft exchanges were cross-checked with the pinned Azalea implementation
listed below. Live application approval, entitlement and online encryption/session
join evidence remain required; fake services do not establish service acceptance.

## Wire and admission contract

One future owns socket reads and writes, preserving compression/encryption
transition ordering and continuous AES-CFB8 state. Three-byte frame lengths,
8 MiB inflated packets, exact zlib completion, a 64 MiB configuration budget,
and bounded NBT depth/node counts prevent unbounded allocation. These are
explicit admission limits, not a claim of full vanilla/custom-pack parity.
Connect and login/configuration have separate deadlines; play has an idle-read
limit instead of a total login deadline. Cancellation drops the stream.

Configuration retains registries, tags and features, requests complete registry
values by accepting no known packs, responds to keepalives/pings/cookies and
acknowledges completion. Play publishes the authenticated profile, server-session
UUID, authoritative entity/world/dimension identities and registry facts, then
position and respawn facts. It answers teleports, keepalives, pings and chunk-batch
flow control. Deferred world/visual packets are explicitly outside admitted state;
the renderer receives no synthetic Bedrock packets or fabricated world.

Unsupported required resource packs, experimental features, transfer, consent
or interactive dialogs, relative teleports before movement integration, mid-session reconfiguration and tag replacement fail
clearly. Chunk/entity/inventory decoding, movement, chat signing, interactions,
Java assets and rendering remain subsequent work. No modded-server or proxy
compatibility is claimed. Unknown state packet IDs are protocol errors.

## Dependency choice and notices

The starter's 15 tests covered status only and were rerun before extension.
Azalea at `0d38e0dcf47eb258c41b1b09ebc2bcde7253e185` was evaluated, but its
protocol target lagged the selected release and required a nightly toolchain and
broader dependencies. Importing its engine was unnecessary. MCProtocolLib at the
manifest revision provides the matching codec reference; it is not a runtime
Java client. No second renderer or game engine is imported.

RustCrypto `aes`, `cfb8`, `rsa`, `sha1`, `md-5` and `rand` supply the cryptographic
primitives; protocol hashes use SHA-1/MD5 because Java specifies them. Production
RSA only encrypts the server challenge; it never decrypts private-key operations.
`flate2` uses its Rust backend, `reqwest` uses rustls, and Hickory supplies DNS.
These are stable-Rust libraries, with exact revisions/checksums in `Cargo.lock`.
Relative to the fork base, the lockfile adds 32 external package/version pairs
(and the two local crates), including DNS, caching and crypto support. Existing
HTTP/TLS packages are reused. No renderer is added. The transport/auth futures
are cancelled by dropping their owning task; queues apply backpressure.
MIT/Apache-2.0 (and the selected alternative licenses recorded by Cargo metadata)
notices must be preserved by packaging; see the repository third-party notices
for the pinned codec reference. The MIT codec reference retains its upstream license. No Mojang code or server
assets are redistributed.
