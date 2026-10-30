# Java authentication acceptance

Linux, Windows, macOS and iOS share the production Microsoft/Xbox/Minecraft
exchange and the fork's default public application ID. A saved `oauth-client.json`
still overrides the default. Desktop credentials use the existing private account
cache; iOS uses Keychain. The desktop cache is not claimed to be an OS vault.

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
