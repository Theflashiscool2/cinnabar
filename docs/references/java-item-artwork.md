# Native Java item artwork

The item resource owner is `assets::registry::java::items::Catalog`. It reads the
pinned archive and native default components, selects Java item-definition trees,
resolves inherited/generated geometry, applies material tints and display transforms,
and prepares GUI pixels on the native inventory artwork worker. The shared session
atlas uploads those pixels to Cinnabar's existing GPU/UI pipeline. Item identities,
counts, cursor state and transaction revisions stay in `inventory::java::State`.

`client-ui/ui_runtime/java_inventory/artwork.rs` bounds pending work to one replaceable
request/result. Session, world epoch, inventory revision, resource catalog identity
and head-resource revision fence publication. Disconnect/replacement closes the old
worker; clearing its atlas cannot clear another edition's atlas.

The native entity exporter now supplies special-item model parts separately from
placed-block transforms. Chest/shulker/banner/skull/statue/conduit/pot/shield/trident
artwork uses those verified parts and the archive's entity textures. Pattern holders
are resolved against the connection's registry, not a guessed numeric ordering.
Potion/effect and pottery defaults come from the same target runtime. Head items
reuse the existing bounded public-profile/skin services, UUID defaults and admitted
local player texture resources. Failed downloads remain diagnostic; a native default
while pending is not successful custom-skin acceptance.

## Reuse and attribution

The isolated MIT resource library at `third_party/java-item-model` vendors selected
unchanged files from `horizon-rust/horizon` revision
`59d8cc38a2627f5b7aa0c96fc91b013df70c45fc`. `UPSTREAM.json` records the exact source
files, SHA-256 hashes and module/manifest adaptations; `LICENSE` preserves its notice.
It owns no network, world, input, inventory or simulation loop.

Special-item pose/submission algorithms in `items/special.rs` are adapted from
`Vasysik/horizon-android` revision `5b8e6d0b6453e57fb23eb7bbb365bbc7edd5b9b2`,
`crates/horizon-client/src/item_render/special.rs`, under that repository's MIT
license. The notice is preserved in `assets/licenses/Native-Java-Reference-MIT.txt`.
The adapter emits Cinnabar quads using Cinnabar's verified model data and material
ownership. No upstream client process or alternate gameplay runtime is embedded.

## Verification and limits

Focused tests cover real generated geometry, inherited textures, count/model
selection, alpha/depth, component identity/removal, independent native potion-colour
vectors, patterned special items and head resources. Set `CINNABAR_JAVA_ASSETS` to a
verified runtime directory to enable the real-archive tests; absent fixtures report
their missing name. Exported data is independently reproduced before updating pins.
A default GUI resource census is structural evidence, not complete item parity.

Linux Xvfb/lavapipe execution against controlled offline vanilla26.3 inspected
ordinary and special inventory icons, patterned banners/pots/shields, head resource
patches, potion mixtures, native default durability, glint and item-model overrides.
Normal client chest use and split/place retained the server item identity and16total
items (8in chest,8in player storage), including unload/revisit and reconnect. Server
commands prepared the fixture; actual client input performed the transfers. An
application-loader regression catches the initially reproduced missing-model binding.
These are functional software-rendering results, not hardware/iOS/Hypixel qualification.

Remaining work includes first-person/remote-equipment integration, owner/world-driven
model predicates and animations, arbitrary pack layering/local head patches outside
the prepared resources, custom special models beyond the integrated factories,
rich tooltip layout and full station menus. GUI thumbnails use the first texture
animation frame. These limits remain implementation gaps; they are not closed by
compilation or the default-resource census. Online, iOS and Hypixel qualification
are separate acceptance gates.
