/** Controlled D presentation inputs on REAL connected players and ordinary native items.
 * This is simulated peer INPUT, not a simulated network player or vanilla-user-input proof.
 * It never creates players, fabricates metadata/collect packets, or awards pickup inventory.
 */
public final class VanillaEntityFixture {
    private static final String TAG = "cinnabar_d_item";

    public static void register(net.minecraft.server.MinecraftServer server) {
        server.getCommands().getDispatcher().register(net.minecraft.commands.Commands.literal("acceptance_d")
            .requires(source -> source.getEntity() == null)
            .then(net.minecraft.commands.Commands.argument("viewer", com.mojang.brigadier.arguments.StringArgumentType.word())
            .then(net.minecraft.commands.Commands.argument("peer", com.mojang.brigadier.arguments.StringArgumentType.word())
            .then(net.minecraft.commands.Commands.argument("phase", com.mojang.brigadier.arguments.StringArgumentType.word())
            .executes(context -> {
                var viewer = server.getPlayerList().getPlayerByName(com.mojang.brigadier.arguments.StringArgumentType.getString(context, "viewer"));
                var peer = server.getPlayerList().getPlayerByName(com.mojang.brigadier.arguments.StringArgumentType.getString(context, "peer"));
                if (viewer == null || peer == null || viewer == peer || viewer.level() != server.overworld()
                        || peer.level() != server.overworld()) throw new IllegalArgumentException("Two distinct real overworld players required");
                var phase = com.mojang.brigadier.arguments.StringArgumentType.getString(context, "phase");
                var level = server.overworld();
                switch (phase) {
                    case "equip" -> {
                        peer.stopUsingItem();
                        peer.setShiftKeyDown(false);
                        peer.setItemSlot(net.minecraft.world.entity.EquipmentSlot.MAINHAND, new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.BOW));
                        peer.setItemSlot(net.minecraft.world.entity.EquipmentSlot.OFFHAND, new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.SHIELD));
                        peer.setItemSlot(net.minecraft.world.entity.EquipmentSlot.CHEST, new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.DIAMOND_CHESTPLATE));
                        var head = new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.PLAYER_HEAD);
                        head.set(net.minecraft.core.component.DataComponents.PROFILE,
                                net.minecraft.world.item.component.ResolvableProfile.createResolved(peer.getGameProfile()));
                        peer.setItemSlot(net.minecraft.world.entity.EquipmentSlot.HEAD, head);
                    }
                    case "bow" -> peer.startUsingItem(net.minecraft.world.InteractionHand.MAIN_HAND);
                    case "sneak" -> peer.setShiftKeyDown(true);
                    case "stop" -> { peer.stopUsingItem(); peer.setShiftKeyDown(false); }
                    case "block" -> { peer.stopUsingItem(); peer.startUsingItem(net.minecraft.world.InteractionHand.OFF_HAND); }
                    case "swing" -> { peer.stopUsingItem(); peer.swing(net.minecraft.world.InteractionHand.MAIN_HAND, net.minecraft.world.item.component.SwingAnimation.DEFAULT, true); }
                    case "drops" -> {
                        for (var entity : level.getAllEntities()) if (entity.entityTags().contains(TAG)) entity.discard();
                        // Existing controlled-world slab/stair. Ordinary gravity, no manual motion ticks.
                        for (double x : new double[]{100.5, 102.5}) {
                            var item = new net.minecraft.world.entity.item.ItemEntity(level, x, 72, 110.5,
                                    new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.IRON_INGOT, 3), 0, 0, 0);
                            item.addTag(TAG);
                            item.setNeverPickUp();
                            level.addFreshEntity(item);
                        }
                    }
                    case "pickup" -> {
                        for (var entity : level.getAllEntities()) if (entity.entityTags().contains(TAG)
                                && entity instanceof net.minecraft.world.entity.item.ItemEntity item) {
                            item.setTarget(viewer.getUUID());
                            item.setNoPickUpDelay();
                            // Move collector with ordinary /tp before this phase. Native playerTouch decides eligibility.
                        }
                    }
                    case "cleanup" -> {
                        peer.stopUsingItem(); peer.setShiftKeyDown(false);
                        for (var entity : level.getAllEntities()) if (entity.entityTags().contains(TAG)) entity.discard();
                    }
                    case "snapshot" -> { }
                    default -> throw new IllegalArgumentException("Unknown D phase");
                }
                var receipt = new com.google.gson.JsonObject();
                receipt.addProperty("phase", phase);
                receipt.addProperty("game_time", level.getGameTime());
                receipt.addProperty("peer_id", peer.getId());
                receipt.addProperty("using_item", peer.isUsingItem());
                receipt.addProperty("remaining_use_ticks", peer.getUseItemRemainingTicks());
                receipt.addProperty("crouching_input", peer.isShiftKeyDown());
                receipt.addProperty("viewer_iron", viewer.getInventory().countItem(net.minecraft.world.item.Items.IRON_INGOT));
                var items = new com.google.gson.JsonArray();
                for (var entity : level.getAllEntities()) if (entity.entityTags().contains(TAG)
                        && entity instanceof net.minecraft.world.entity.item.ItemEntity item) {
                    var row = new com.google.gson.JsonObject();
                    row.addProperty("id", item.getId()); row.addProperty("count", item.getItem().getCount());
                    row.addProperty("x", item.getX()); row.addProperty("y", item.getY()); row.addProperty("z", item.getZ());
                    row.addProperty("on_ground", item.onGround()); row.addProperty("age", item.getAge());
                    items.add(row);
                }
                receipt.add("items", items);
                context.getSource().sendSuccess(() -> net.minecraft.network.chat.Component.literal(receipt.toString()), false);
                return 1;
            })))));
    }
}
