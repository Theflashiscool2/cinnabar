# Java authentication acceptance

Linux, Windows, macOS and iOS share the production Microsoft/Xbox/Minecraft
exchange and the fork's default public application ID. A saved `oauth-client.json`
still overrides the default. Desktop credentials use the existing private account
cache; iOS uses Keychain. The desktop cache is not claimed to be an OS vault.

Normal joins capture the selected Java profile when Join is pressed. If selection
changes or that account is removed before preparation, the join fails explicitly
instead of silently adopting another account. Existing session monitoring stops a
connection when the selected account changes. The developer loopback offline
fixture is a separate entry point and cannot override an account-backed join.
Account-list actions retain the displayed profile identity across concurrent disk
changes. These protections have source-level fixture coverage; they do not prove
an authenticated service exchange or iOS Keychain operation on a device.

The exact Mac build, asset preflight, packaging and controlled online procedure is
in [the iOS Java test handoff](ios-java-test.md). No new registration is required for
the already configured and owner-approved application. This execution environment
has no legitimate account credentials and denies the authentication hosts; its
GitHub runner API request is also denied. The next online test needs permitted
Microsoft/Xbox/Minecraft endpoints and one interactive login by the account owner.

For isolated desktop authentication without renderer/assets, from the checkout:

```sh
cargo run --locked -p java-protocol --example auth_probe -- 76e54634-fa88-4777-ae76-41d004eecebc
```

Open the displayed Microsoft address, enter the device code and authorize your
own Java account. The probe uses the real production exchange, checks entitlement
and profile, and discards credentials. It does not connect to a server or establish
GUI, Windows or iPhone acceptance. Ctrl+C stops the process. No passwords or tokens
are command-line arguments. Do not share the temporary device code.

The app emits `java_auth` tracing events into its configured logging sink: operation,
service stage, HTTP status, and bounded Minecraft rejection diagnostics. No raw
responses, bearer tokens, authorization headers, refresh tokens or device secrets
are logged. Unknown error text is deliberately excluded. Preserve the stage/status,
symbolic code and request UUID when reporting an error.

This workspace's proxy currently denies the Microsoft login host (CONNECT 403),
which is an environment failure before Microsoft handles the request. The owner
reports Minecraft API allowlist approval; this proxy failure is unrelated to that
approval and to the earlier iPhone Minecraft-service HTTP 403.
