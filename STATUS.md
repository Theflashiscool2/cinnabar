# Native Java gameplay status

Baseline: **8e713eabf**, Java **26.3 / protocol 777**. The approved handoff change from **ffef71e81** is integrated as **c1a7e2ffe** on the feature branch. A/C/D gameplay remains unmerged; candidate evidence below does not mean a completed Bed Wars port.

States: `todo` = outstanding implementation or acceptance; `in progress` = active work; `candidate` = implemented on the named unmerged review branch; `done` = implemented with focused evidence; `verified` = stated acceptance scope passed; `blocked` = named external dependency. A zero counter, missing sample, queue write, or sequence ACK alone is not acceptance. No row here implies iPhone/Hypixel qualification.

The already implemented slime bounce, double-tap sprint, ordinary inventory movement, basic chat, native ordinary mining/use/swings, spatial grass, and invalid-angle handling are preserved. Existing evidence is retained in `docs/tracking/java-iphone.md` and the private recovery checkpoint; some historical entries in that document are superseded by later entries. This table tracks the remaining checklist, not a claim those working systems are absent. In particular, ordinary bare-arm/swing rendering already exists; B01 is its remaining completeness. Baseline controlled fist/sword/wool, biome/reconnect and server damage evidence does not close the broader device/server flow Checks below.

**Approval gates:** telemetry/replay first; then three separate A/C/D branches/worktrees/subagents in parallel. B/E/F held until A/C/D merges are approved. Infrastructure may merge after checks; every gameplay lane requires vanilla-reference tests, replay comparison and explicit owner approval before integration. Flight/mounts last in A. No unfinished movement/interaction tests on Hypixel. Resource thresholds used only for diagnostics are not vanilla gameplay constants.

## Current review state — 2026-11-11

Approved handoff source `ffef71e81` was applied as **c1a7e2ffe**, without merging its unapproved A/C ancestry. Only that gameplay change has approval. The handoff residuals and all 28 skipped-fixture diagnostics remain below.

Exactly one current candidate per lane, rebased on **057fea8e2** (the feature codebase including the approved handoff; subsequent reporting changes are documentation only):

| Lane | Current remote branch / commit | Verification / gate |
| --- | --- | --- |
| A movement | `candidate/java-lane-a` / **e88830a44** | Exact native build PASS; 9/9 fresh controlled flight/authority checks including in-window double Jump; complete two-connection capture; unchanged replay comparisons 22/22 + 22/22. Focused domain/reference evidence reused only for identical source. Local verification complete; owner merge approval pending; no new feature work. Mounts remain unimplemented. |
| C interaction | `candidate/java-lane-c` / **ea5ca9851** | **C00 review state: candidate**, not merged. Fresh server-confirmed placement/mining/denial, Adventure permission checks, transfer/reconnect and isolated ACK rollback PASS. Four replay comparisons 22/22. Retained ffef binary reused with production/build-input byte identity recorded; no fresh C compile claim. Local verification complete; owner merge approval pending. Broader predicate/callback gaps remain below. |
| D entities | `candidate/java-lane-d` / **4d55708a9** | Parked until A/C land. Source/core/fixture candidates consolidated; Rust/native two-peer acceptance remains pending. Proposed old publication-lifetime patch `7560097ab` conflicts with newer optional-head ownership and remains separately preserved for evaluation; unwired fluid WIP `94f5df85f` also retained. Neither is silently integrated. |

All candidate branches are pushed. All original branch tips/worktrees and private evidence are retained; no branch/worktree was deleted. The private `BRANCH-CONSOLIDATION.md` individually classifies every other branch as superseded, still needed or safe to delete (ref only). A/C agents worked in separate verification worktrees; expensive builds and graphical sessions were serialized. No lane agents remain running after verification. B/E broader and F remain held under the existing approval rules.

The assistant owns controlled local acceptance; the owner need not repeat it. The combined unmerged candidate **284101dfd6d8df8eb58f95840566b065eb04fe50** is cleared for normal owner-directed Hypixel compatibility testing, not certified Hypixel support. See [candidate test instructions](docs/testing/java-hypixel-candidate.md). The [local run sheet](docs/testing/java-ac-local.md) remains optional debugging reference. Gameplay merges still require explicit owner approval.

### Evidence and qualifications

Combined candidate: Linux native graphical flight 9/9; placement/mining/denial, Adventure permissions, isolated real-ACK rollback, NPC/shop purchase, transfer and reconnect passed against controlled vanilla. Seven replay comparisons each passed 22/22; seven focused transport tests passed. Server shop inventory changed iron 8→7 and wool 16→20. Screenshots inspected. Formatting and architecture checks passed. Native executable compiled at17ee59d58; candidate284101dfd changes only fixture cleanup, with production/build inputs identical. Initial combined build caught a duplicate identical shulker-tick method; removed once. Failed rollback/shop fixture attempts are retained: collision was sampled after input release, and leftover dropped wool contaminated shop counts. Corrected fixture setup retained the original assertions. Full workspace/MCP coverage remains incomplete.


- A exact native build: 55m57s, with explicit disk reserve. Fresh A telemetry: four authored teleports plus one 0.0784 m server reaction at a Spectator boundary; reconnect has zero corrections. The old525 baseline sent the identical first downward Spectator movement. Pinned vanilla preserves inherited velocity; server tick/collision timing is a supported explanation, but its runtime noPhysics flag was not recorded. Do not label all five corrections authored or claim zero corrections.
- A max captured frame interval 7.753s (pending texture); tick-end packets continued, largest gap106.010ms, p99 58.092ms. Max main schedule34.796ms; backlog0 at named checkpoints. These are Linux lavapipe observations, not representative iPhone performance or proof of perfect cadence. One early Spectator screenshot had a black sky; immediate/settled repeat screenshots were normal. I03 first-use graphics readiness remains open.
- The first A fixture incorrectly demanded flying remain enabled near the ground when changing Spectator→Creative. Exact vanilla26.3 clears it within its ground-range rule. The corrected authored fixture uses real Spectator ascent before changing mode; all original checks/assertions remain. Near-ground revocation is expected behavior. The first replay loader also failed before playback on normalization metadata; a snapshot-type filter corrected the private loader with assertions unchanged. Failures retained.
- C00 fresh telemetry: 3 predictions, 2 confirmations, 1 controlled denial revert, 4 ACKs; no listed queue, stale-click, lifecycle or active-hand-publication anomaly. Adventure: 2 predictions/2 confirmations; denied mining sends no START; denied placement preserves authoritative count.
- C isolated test delivered real block ACKs while delaying only the later actual teleport for228.63ms: body returned from z104.699999988 to saved z102.5, correction count unchanged. The real teleport was then released, honored at z103.542430928 and acknowledged with its matching ID. Stationary-motion preconditions measured; no fake ACK/on-ground flags or ignored correction.
- C first proxy-refusal attempt remains preserved and unexplained; the diagnostic repeat completed with no capture errors. Initial Adventure replay comparisons passed but its driver disconnected before transcript completion; explicit completion waits fixed the private harness, not the client, with original assertions unchanged.

Private detailed evidence: `candidate-{a,c}-approval-evidence.json`, `candidate-{a,c}-built-binary.json`, `lane-a-e888-small-correction-review.json`, and fresh `runtime/evidence/lane-a-e888-{live2,replay3}/`, `candidate-c-ea5-{live2,adventure,isolated-rollback}/`. Fresh live and replay screenshots were inspected. Telemetry/replay infrastructure already merged at2dd74fe6c; no A/C/D gameplay merge performed.

Disk cleanup reclaimed9,596,280,892bytes of rebuildable metadata/alternate library cache, then the build guard reclaimed5,121,757,511bytes of completed incremental cache. Free space after acceptance: about5.8GiB. Guard requires6GiB before build, trims only completed unlocked increments below4GiB and stops before2.5GiB; accepted binaries/assets/worlds/private accounts/logs/backups remain intact.

Scope: native Java26.3/protocol777, Linux hidden Xvfb/Mesa lavapipe controlled vanilla and inbound replay. No new authenticated-online, Hypixel, Mac, simulator or physical-iPhone acceptance. No new IPA. Full app/workspace/MCP suite remains incomplete. Full unfiltered client-ui test on ffef:1,760passed,0failed,4ignored,0filtered; named missing-fixture returns limit its coverage as listed below.

## Owner Hypixel Mac observation — combined candidate284101dfd

Owner reports lobby, NPC/game menus, backend transitions, Bed Wars shops, ordinary inventory, placement/mining, combat and return/reconnect working. Entered/played Bed Wars, regular iron1v1 Duels and Dropper. These are owner-reported Mac observations after the candidate build instructions, without a new diagnostic archive or independent frame/server trace yet; they do not establish all match mechanics, death/respawn, or physical-iPhone acceptance. Death/respawn remains unverified. This report is not explicit gameplay merge approval.

| Reported defect | Owning lane | State / required evidence |
| --- | --- | --- |
| Empty-hand block-contact swing absent; air swing works | B/C | Open: compare mining admission and native startAttack/continueAttack animation behavior; do not bypass server restrictions. |
| Shift-click makes an item disappear | C/UI | Open: reproduce quick-move with authoritative source/destination/cursor state, then close/reopen. Current observed-action path does not predict movement; actual loss versus stale display is unknown. |
| Placed heads and End portal surface flicker | D/render publication | Open: distinguish alternating geometry/material publication from server state changes; preserve entity lifetimes and supported specialized dispatch. |
| Bow and fishing rod do not work | C/D | Open: determine use press/hold/release, selected item/ammo and real server response from diagnostics; no cause yet established. |
| Duels launch pressure plate leaves player stuck | A | Open: capture impulse/corrections/collision and mode/flight state; no inferred anti-cheat cause. |
| Dropper portal transition leaves player floating/stuck | A/C/session | Open: separate collision, server transfer and loading gates from portal-surface rendering. |
| Map loading remains very slow | streaming/presentation | Open: measure stages on owner's Mac; Linux lavapipe timings do not establish the Mac or iPhone cause. |

Next: obtain private diagnostic archive for this run, reproduce safe controlled-server counterparts (inventory first), and propose fixes on unmerged candidate branches. No public automated gameplay. Gameplay merges retain owner approval gate; D remains parked pending that approval.

## Open handoff residuals

| ID | Type | Lane | State | Remaining behavior | Evidence / next gate |
| --- | --- | --- | --- | --- | --- |
| B10 | Gap | B/E narrow handoff | todo | Cold held-item geometry on immediate reconnect | Exact ffef replay-reconnect screenshot/state has no held geometry while cold artwork prepares. Stress passing does not close this gap. |
| B11 | Gap | B/E | todo | Broader equip completeness | Native equip owner, cold first-use and full equip/unequip behavior are not established by scoped resource-handoff tests. B06 remains partial. |

## Full client-ui run: skipped fixture diagnostics

The unfiltered ffef run reports 1,760 passed, 0 failed and 4 explicitly ignored tests. In addition, these **28 named skip diagnostics** returned early for absent fixtures/opt-in benchmarks. They are coverage limitations, not 28 completed visual checks. This table records the exact reasons from the retained log; no fixtures were excluded to obtain a passing result.

| # | Fixture/check diagnostic | Reason |
| --- | --- | --- |
| 1 | snapshot_component_weapon_anvil | missing CINNABAR_ANVIL_CAPTURE_CARRIER_DIR |
| 2 | admitted_dynamic_title_selects_only_its_authored_branch | missing CINNABAR_TEST_DYNAMIC_HUD_DIR fixture |
| 3 | admitted_effects_publish_from_raw_messages | missing CINNABAR_TEST_EFFECT_PACK_DIRS fixture |
| 4 | retained_inventory_animation_advances_and_repeats_two_loops | missing CINNABAR_INVENTORY_ANIMATION_LOOP_MS fixture |
| 5 | admitted_retained_title_panel_publishes_and_survives_other_titles | missing CINNABAR_TEST_RETAINED_HUD_DIR fixture |
| 6 | admitted_settings_forms_keep_every_interactive_element | missing original pack/form fixtures |
| 7 | routed_custom_form_retains_its_label_and_toggle | missing CINNABAR_TEST_FORM_UI_DIR fixture |
| 8 | admitted_hud_hides_its_authored_title_and_actionbar_triggers | missing CINNABAR_TEST_HUD_UI_DIR fixture |
| 9 | compact_font_snapshots | missing CINNABAR_FONT_SNAPSHOT_CARRIER fixture |
| 10 | installed_vanilla_and_zeqa_palettes | missing .local/resource-packs/v1/objects |
| 11 | zeqa_loading_pixels_survive_artwork_repacking_and_pack_reload | fixture unavailable; requires installed local carriers (make assets) |
| 12 | zeqa_loading_screen_animates_the_vanilla_bar_when_not_overridden | fixture unavailable; requires installed local carriers (make assets) |
| 13 | server_pack_selector_keeps_reserved_controls_out_of_game_grid | missing CINNABAR_FORM_PACK_DIR fixture |
| 14 | snapshot_pack_image_grid | fixture unavailable; requires installed UI carrier (make assets) and CINNABAR_FORM_PACK_DIR |
| 15 | training_labels_keep_practice_before_the_ellipsis | fixture unavailable; requires installed UI carrier (make assets) and CINNABAR_FORM_PACK_DIR |
| 16 | font MOTD snapshot | missing CINNABAR_FONT_MOTD_FIXTURE |
| 17 | font parity snapshots | missing CINNABAR_FORM_SNAPSHOT_DIR |
| 18 | local_server_ui_publication_profile | CINNABAR_UI_PUBLICATION_CARRIER, CINNABAR_UI_PUBLICATION_FONT and CINNABAR_UI_PUBLICATION_PACKS |
| 19 | spirit_bundle_before_pack_has_stable_resident_pages_after_install | fixture unavailable; requires installed UI carrier, CINNABAR_LOBBY_CAPTURE and CINNABAR_FORM_PACK_DIR |
| 20 | spirit_bundle_snapshot | fixture unavailable; requires installed UI carrier, CINNABAR_LOBBY_CAPTURE and CINNABAR_FORM_PACK_DIR |
| 21 | hud_cosmetic_publication_cost | opt-in HUD motion benchmark; set CINNABAR_HUD_MOTION_BENCH=1 |
| 22 | admitted_item_font_retains_its_spacing_and_count_glyphs | missing CINNABAR_FORM_PACK_DIR fixture |
| 23 | snapshot_local_pack_hud | missing CINNABAR_FORM_PACK_DIR fixture |
| 24 | offline_server_hud_publication_cost | missing offline server pack directories |
| 25 | server_pack_stack_hud_dump | fixture unavailable; requires installed local carriers (make assets) and CINNABAR_HUD_PACK_STACK |
| 26 | server_packs_restyle_the_engine_hud | fixture unavailable; requires installed UI carrier (make assets) and CINNABAR_HUD_PACK_DIRS |
| 27 | zeqa_top_bar_snapshot | fixture unavailable; requires installed local carriers (make assets) and CINNABAR_HUD_PACK_STACK |
| 28 | loading_screen_pack_snapshot | fixture unavailable; requires installed local carriers (make assets) |

Four separately ignored tests: `mod_spike_offline_frame_overhead` (real carrier + compiled sample); `frame_cost_bench_nametag_updates` (release measurement); `hud_frame_timing` (manual carrier-backed HUD benchmark); `frame_cost_bench_hidden_player_preview_while_turning` (benchmark).

| ID | Type | Lane | State | Required behavior | Owning implementation (`crates/` or `app/src/`) | Evidence / next gate |
| --- | --- | --- | --- | --- | --- | --- |
| A01 | Gap | A | done | Honey slide/stick/contact callbacks | sim/src/java/travel; gameplay/src/java; session/java_movement | Unmerged A candidate: version-reference/unit and earlier lane-a-c51 controlled honey evidence retained; not iPhone/Hypixel acceptance. |
| A02 | Gap | A | done | Cobweb movement callbacks | sim/src/java/travel; gameplay/src/java; session/java_movement | Unmerged A candidate: version-reference/unit and earlier lane-a-c51 controlled cobweb evidence retained; not iPhone/Hypixel acceptance. |
| A03 | Gap | A | done | Attribute-driven bounce | sim/src/java/travel; gameplay/src/java; session/java_movement | Unmerged A candidate: attribute-aware bounce implementation/reference and prior controlled evidence retained; integrated device acceptance pending. |
| A04 | Gap | A | candidate | Server-granted flight (last within A) | sim/src/java/travel; gameplay/src/java; session/java_movement | Current e888 candidate: 9/9 fresh live checks (including valid-window manual double Jump), native build and 22/22 +22/22 replay. Vanilla ground-range transition and one boundary correction qualified above. Mac/owner approval pending. |
| A05 | Gap | A | todo | Mounted movement (last within A) | sim/src/java/travel; gameplay/src/java; session/java_movement | Targeted mounted audit identifies missing D seat/attribute/boost facts plus A vehicle transport; no mounted implementation or acceptance claim. |
| A06 | Check | A | in progress | Standing/crouch/walk/sprint/jump transitions | sim/src/java/travel; gameplay/src/java; session/java_movement | Earlier movement/flight fixture evidence covers limited transitions; full sweep and latest A core graphical acceptance pending. |
| A07 | Check | A | todo | Sprint-jump and crouch-walk jitter | sim/src/java/travel; gameplay/src/java; session/java_movement | Pending scoped test/live evidence. |
| A08 | Check | A | todo | Sneaking at edges and bridging | sim/src/java/travel; gameplay/src/java; session/java_movement | Pending scoped test/live evidence. |
| A09 | Check | Integration | todo | Sprint FOV transition/settings; integration owner | sim/src/java/travel; gameplay/src/java; session/java_movement | Pending scoped test/live evidence. |
| A10 | Check | A | todo | Slab/stair/fence/trapdoor/tight-corner collision | sim/src/java/travel; gameplay/src/java; session/java_movement | Pending scoped test/live evidence. |
| A11 | Check | A | in progress | Movement after spawn/teleport/respawn/transfer | sim/src/java/travel; gameplay/src/java; session/java_movement | C prediction/adventure transfer/reconnect and 525 flight fixtures cover limited boundaries. Latest A core app test and full respawn/transfer sweep pending. |
| B01 | Gap | B (held) | todo | Remaining bare-arm composition and special attack poses | session/java_hands; java_interaction/swing; shared hand pass | Ordinary hand exists. ffef fixes one resource-handoff failure; remaining bare-arm/special poses and cold geometry remain incomplete. |
| B02 | Gap | B (held) | todo | Remaining active-use held poses | session/java_hands; java_interaction/swing; shared hand pass | Pending scoped test/live evidence. |
| B03 | Check | B (held) | in progress | Repeated empty-hand and held swings | session/java_hands; java_interaction/swing; shared hand pass | ffef full client-ui run passes; unchanged 10x replay has 0 active hand-publication losses. Not a complete repeated empty/held swing or GPU/device sweep. |
| B04 | Check | B (held) | in progress | Placement gesture matches accepted placement | session/java_hands; java_interaction/swing; shared hand pass | ffef unchanged replay exercises real client placement input and displays held wool; replay does not prove new server action acceptance or every placement gesture. |
| B05 | Check | B (held) | in progress | Held attack, changing target and slot mid-swing | session/java_hands; java_interaction/swing; shared hand pass | ffef original 10x tool/wool switch stress passes 22/22 transfer + 22/22 reconnect; active hand-publication loss 0. Broader target-change/device sweep pending. |
| B06 | Check | B (held) | in progress | Equip/unequip transitions | session/java_hands; java_interaction/swing; shared hand pass | ffef selected-hand resource handoff passes full UI suite and original stress; immediate reconnect has no held geometry while cold artwork prepares. Native equip owner remains incomplete. |
| B07 | Check | B (held) | todo | Empty/tool/block hand positioning | session/java_hands; java_interaction/swing; shared hand pass | Pending scoped test/live evidence. |
| B08 | Check | B (held) | todo | Eat/drink/bow release and cancellation visuals | session/java_hands; java_interaction/swing; shared hand pass | Pending scoped test/live evidence. |
| B09 | Check | B (held) | todo | Mining cracks/particles/sounds/cancellation feedback | session/java_hands; java_interaction/swing; shared hand pass | Pending scoped test/live evidence. |
| C00 | Gap | C | candidate | TOP PRIORITY: version-matched placement/break prediction, sequence-ACK reconciliation and revert telemetry | gameplay/java interaction + world/terrain + publication; confirm vanilla protocol 777 first | Candidate on candidate/java-lane-c ea5ca9851: real placement/break ACK prediction, denied revert, independent server stack/block checks, transfer/reconnect and pre-teleport saved-body rollback verified. Four22/22 replay comparisons. Not merged. Broader specialized callbacks remain C01 gaps. |
| C01 | Gap | C | in progress | Remaining specialized use and interaction-result fallback | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Ordinary prediction/use paths implemented; broad specialized callbacks/fallback coverage still incomplete. No full vanilla-use claim. |
| C02 | Gap | C | in progress | Selected same-item component refresh | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Unmerged component-refresh candidate and prior scoped evidence retained; copied-component alias identity/tag rebind remains unresolved. |
| C03 | Gap | C | in progress | Adventure can_break predicates | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | 8d Adventure controlled cases/reference tests accepted; full block-entity metadata NBT predicates remain incomplete. |
| C04 | Check | C | in progress | Repeated breaking of placed Bed Wars wool | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Prior controlled wool mining and ffef delayed-ACK replay pass; extended held/repeated Bed Wars/device sweep pending. |
| C05 | Check | C | todo | Mining cancellation on release/look/menu/slot boundaries | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Pending scoped test/live evidence. |
| C06 | Check | C | todo | Native tool/effect/airborne/underwater break speed | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Pending scoped test/live evidence. |
| C07 | Check | C | todo | Rapid bridging without ghost/misplaced blocks | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Pending scoped test/live evidence. |
| C08 | Check | C | todo | Placement against partial and interactive blocks | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Pending scoped test/live evidence. |
| C09 | Check | C | todo | Both-hand use/place/sneak precedence | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Pending scoped test/live evidence. |
| C10 | Check | C | in progress | Denied mining/placement authoritative recovery | gameplay/src/java/{interaction,mining,attack,item_use}; session/java_interaction | Controlled denied prediction restoration/collision rollback evidence retained; ffef original stress admits denial revert and transfer/reconnect without anomaly. Broader denied-action sweep pending. |
| P01 | Check | C (A/D/F dependent acceptance) | todo | Fireball use/consumption/flight/impact/knockback | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P02 | Check | C (A/D/F dependent acceptance) | todo | Fireball jumping | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P03 | Check | C (A/D/F dependent acceptance) | todo | TNT placement/fuse/explosion/knockback | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P04 | Check | C (A/D/F dependent acceptance) | todo | Pearl use/teleport/arrival movement | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P05 | Check | C (A/D/F dependent acceptance) | todo | Bow charge/release/arrows/hit feedback | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P06 | Check | C (A/D/F dependent acceptance) | todo | Potions/milk/golden apple consumption/effects | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P07 | Check | C (A/D/F dependent acceptance) | todo | Spawn eggs and mode-specific utility items | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P08 | Check | C (A/D/F dependent acceptance) | todo | Attacks against moving players during slow frames | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P09 | Check | C (A/D/F dependent acceptance) | todo | Sprint/airborne attacks, timing and accepted damage | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P10 | Check | A | todo | Knockback without jitter/sticking/bad correction | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| P11 | Check | Integration | todo | Death/spectator/countdown/respawn/restored play | gameplay/java; native entities; player travel; session lifecycle | Pending scoped test/live evidence. |
| D01 | Gap | D | in progress | Remaining equipment/head/trim layers and non-idle poses | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | e44 core and a8 fixture candidates pushed; source/reference/Java fixture evidence only, owning Rust and graphical acceptance pending. |
| D02 | Gap | D | in progress | Dropped-item physics in fluids and special/dynamic shapes | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Independent fluid helper preserved remotely at94f5df85f, not wired/compiled; narrow A/D integration approval remains pending. |
| D03 | Gap | D | in progress | Pickup animation | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | D pickup candidate pushed; production/native two-peer graphical acceptance still pending. |
| D04 | Check | D | todo | Custom standard/slim/legacy skins | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| D05 | Check | D | todo | NPC names and invisible-stand holograms | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| D06 | Check | C | todo | NPC action opens game/shop menu; C owns request | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| D07 | Check | D | todo | Remote pose/swing/use/equipment updates | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| D08 | Check | D | todo | Remote interpolation under slow rendering | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| D09 | Check | D | todo | Dropped-item spawn/motion/merge/pickup presentation | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| D10 | Check | D | todo | Despawn/reused IDs/transfer/reconnect cleanup | world/src/java/entities; client-world/src/java/entities; sim/java/item; session/java_entities,java_heads | Pending scoped test/live evidence. |
| E01 | Gap | E (held) | todo | Rarity/enchantment/effect/advanced tooltip details | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E02 | Gap | E (held) | todo | Remaining owner/world item predicates and animated textures | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E03 | Check | E (held) | todo | Menu icons stable through updates | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E04 | Check | E (held) | todo | Purchases reconcile currency/items/equipment | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E05 | Check | E (held) | todo | Rapid clicks and menu replacement | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E06 | Check | E (held) | todo | Shift/split/drag/number-swap/drop gestures | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E07 | Check | E (held) | todo | Full inventory/rejected purchase/cursor recovery | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| E08 | Check | E (held) | todo | Heads/names/lore/prices/durability/glint | inventory/src/java; client-world/java/inventory; client-ui/java_inventory and item presentation | Pending scoped test/live evidence. |
| F01 | Gap | F (held) | todo | Remaining signed prefixes/rich text/suggestions | java-protocol/session/chat; client-world/java; java-resource-pack; session/java_chat,java_hud,java_service | Pending scoped test/live evidence. |
| F02 | Gap | F (held) | todo | Full Java resource-pack application/status/cleanup | java-protocol/session/chat; client-world/java; java-resource-pack; session/java_chat,java_hud,java_service | Pending scoped test/live evidence. |
| F03 | Check | F (held) | todo | Normal/party/team/private chat | java-protocol/session/chat; client-world/java; java-resource-pack; session/java_chat,java_hud,java_service | Pending scoped test/live evidence. |
| F04 | Check | F (held) | todo | Sidebar/teams/names/tab/titles/actionbar/bossbar | java-protocol/session/chat; client-world/java; java-resource-pack; session/java_chat,java_hud,java_service | Pending scoped test/live evidence. |
| F05 | Check | F (held) | todo | Lobby/game/respawn/lobby HUD/loading transitions | java-protocol/session/chat; client-world/java; java-resource-pack; session/java_chat,java_hud,java_service | Pending scoped test/live evidence. |
| F06 | Check | F (held) | todo | Account continuity and reconnect | java-protocol/session/chat; client-world/java; java-resource-pack; session/java_chat,java_hud,java_service | Pending scoped test/live evidence. |
| I01 | Check | Integration | todo | Essential spawn terrain/collision readiness | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| I02 | Check | Integration | todo | Progressive skin/item loading without network freeze | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| I03 | Check | Integration | in progress | Cold loading/upload queues/menu responsiveness | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| I04 | Check | Integration | todo | Touch attack/use/held mining/bridging/sprint/hotbar | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| I05 | Check | Integration | todo | Touch inventory drag/split | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| I06 | Check | Integration | todo | Keyboard/chat submit/dismiss and input release | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| I07 | Check | Integration | todo | Safe areas/orientation/background/resume/reconnect | session/java_render/loading,java_entities; semantic_controls; ui_runtime/gameplay_touch; packaging/ios | Pending scoped test/live evidence. |
| T01 | Gap | Telemetry | done | Corrections/context/magnitude | client-session/java/telemetry; passive app hooks | Done in step 1. App tests: observed_correction_uses_resolved_relative_position_and_preserves_authority; live on-report: 3 corrections, initial position separate. Manual server teleports are expected, not physics failures. |
| T02 | Gap | Telemetry | done | Block prediction coverage/ACKs/reconciliation | client-session/java/telemetry; passive app hooks | Infrastructure records sequence ACKs, updates, prediction confirmations/reverts. Public feature lacks C gameplay merge; unmerged C/ffef records 3 predictions, 2 confirmations, 1 denial revert in unchanged stress. |
| T03 | Gap | Telemetry | done | Container refusal/stale/write receipts | client-session/java/telemetry; passive app hooks | Done in step 1. Bounded-queue/report tests; actual pointer pickup/place: 2 writes, server inventory retains 7 stone in slot 1. Live stale/rejected shop coverage E05/E07 remains pending. |
| T04 | Gap | Telemetry | done | Swing/use cancellation and publication loss | client-session/java/telemetry; passive app hooks | Recorder/cause-history tests retained. ffef unchanged 10x replay: active_animation_lost_hand_publication=0 versus one empty-geometry loss on8d; full UI suite passes. CPU publication is not GPU visibility or accepted damage. |
| T05 | Gap | Telemetry | done | Entity lifecycle mismatches/collisions | client-session/java/telemetry; passive app hooks | Done in step 1. entity_collision_and_missing_despawn_are_observed_without_weakening_lifetimes; live ordinary spawn/removal observed. Full D10 lifecycle sweep pending. |
| T06 | Gap | Telemetry | done | Frame/main-schedule/loading CPU scopes | client-session/java/telemetry; passive app hooks | Done in step 1. Scoped-timing tests; live 443 terrain/entity/hand scope samples, 442 frame/main-window samples. Threshold diagnostic only; GPU/hardware/iPhone performance unqualified. |
| H01 | Gap | Harness | done | Capture/replay through normal admission with world/HUD/inventory assertions | Existing native protocol/session/world owners | tools/java-replay: 13 focused tests; actual Linux vanilla shop/transfer capture + native input replay, 22 state/anomaly assertions pass. Scope: phase, columns/entities, full storage, HUD, menu; not every-block/skin/online acceptance. Inspection hold uses real native replies to fixture keepalives; actual38s hold remained Play with2replies. |
| H02 | Gap | Harness | done | One short compatible Paper/Bed Wars build/plugin verification; otherwise vanilla test world | Controlled harness | One bounded Paper/BedWars1058 metadata request each: HTTP403, compatibility unestablished. Verified vanilla fallback world + real server-owned villager/chest shop and hologram used graphically. No Bed Wars plugin claim. |

Step 1 evidence: 9 recorder tests, 2 app admission tests, 4 existing schedule tests pass; private captures/reports under `runtime/evidence/telemetry-{on,off}` in the recovery workspace. Normal pointer UI confirmed after the developer inventory shortcut refused its request; that shortcut is not a passing transfer. No Hypixel, authenticated-online, iOS, or full-workspace-suite acceptance is claimed. Telemetry merge e26e6f121 was approved and pushed. Infrastructure-only merges are authorized after checks; gameplay merges still require approval.
