# Native command parser provenance

The Rust child-selection and redirect traversal adaptation is pinned in UPSTREAM.json.
LICENSE retains that source's MIT notice. BRIGADIER-LICENSE also retains the MIT
notice from the official Mojang/brigadier repository's LICENSE (retrieved2026-11-05).
Cinnabar's own protocol decoder and signing state remain authoritative; this port does
not instantiate an upstream world, player simulation, inventory or connection.

All pinned parser property formats are decoded. Local signing supports ordinary
message commands and aliases; complex selectors, UUID targets and other argument
parsers before message arguments remain explicit limitations. An insecure-chat server
may still receive the existing unsigned command, with a visible diagnostic. This is
not an authentication fallback and does not supply or invent signatures.
