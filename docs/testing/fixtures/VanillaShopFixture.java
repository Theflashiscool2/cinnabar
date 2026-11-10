/** Authored controlled-server shop, NOT a Hypixel implementation or compatibility claim.
 * A real villager interaction opens a real chest menu. The server charges inventory currency
 * and grants the result; no client-side purchases or packet injection are involved.
 */
public final class VanillaShopFixture {
    public static void register(net.minecraft.server.MinecraftServer server) {
        server.getCommands().getDispatcher().register(net.minecraft.commands.Commands.literal("acceptance_shop")
            .requires(source -> source.getEntity() == null)
            .executes(context -> {
                var level = server.overworld();
                for (var entity : level.getAllEntities()) {
                    if (entity.entityTags().contains("cinnabar_shop")) entity.discard();
                }
                var shop = new Shop(level);
                shop.setPos(104.5, 69, 107.5);
                shop.setNoAi(true);
                shop.setPersistenceRequired();
                shop.setCustomName(net.minecraft.network.chat.Component.literal("Fixture Shop — 4 wool / 1 iron"));
                shop.setCustomNameVisible(true);
                shop.addTag("cinnabar_shop");
                level.addFreshEntity(shop);
                return 1;
            }));
    }

    private static final class Shop extends net.minecraft.world.entity.npc.villager.Villager {
        Shop(net.minecraft.world.level.Level level) {
            super(net.minecraft.world.entity.EntityTypes.VILLAGER, level);
        }

        @Override
        public net.minecraft.world.InteractionResult mobInteract(net.minecraft.world.entity.player.Player player,
                net.minecraft.world.InteractionHand hand) {
            if (hand != net.minecraft.world.InteractionHand.MAIN_HAND) return net.minecraft.world.InteractionResult.PASS;
            player.openMenu(new net.minecraft.world.SimpleMenuProvider((id, inventory, owner) -> {
                var stock = new net.minecraft.world.SimpleContainer(27);
                stock.setItem(13, new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.WOOL.white(), 4));
                return new net.minecraft.world.inventory.ChestMenu(net.minecraft.world.inventory.MenuType.GENERIC_9x3,
                        id, inventory, stock, 3) {
                    @Override
                    public void clicked(int slot, int button, net.minecraft.world.inventory.ContainerInput input,
                            net.minecraft.world.entity.player.Player buyer) {
                        if (slot != 13 || button != 0 || input != net.minecraft.world.inventory.ContainerInput.PICKUP) {
                            // Player slots remain ordinary vanilla inventory actions; shop cells aren't takeable.
                            if (slot >= 27) super.clicked(slot, button, input, buyer);
                            else sendAllDataToRemote();
                            return;
                        }
                        var items = buyer.getInventory();
                        for (int i = 0; i < items.getContainerSize(); ++i) {
                            var currency = items.getItem(i);
                            if (!currency.is(net.minecraft.world.item.Items.IRON_INGOT)) continue;
                            var result = new net.minecraft.world.item.ItemStack(net.minecraft.world.item.Items.WOOL.white(), 4);
                            // Preflight native stack capacity: add() can partly mutate before returning false.
                            int capacity = 0;
                            for (var stack : items.getNonEquipmentItems()) {
                                if (stack.isEmpty()) capacity += result.getMaxStackSize();
                                else if (net.minecraft.world.item.ItemStack.isSameItemSameComponents(stack, result))
                                    capacity += Math.max(0, stack.getMaxStackSize() - stack.getCount());
                            }
                            if (capacity < result.getCount()) {
                                sendAllDataToRemote();
                                return;
                            }
                            if (items.add(result)) {
                                currency.shrink(1);
                                items.setChanged();
                            }
                            broadcastChanges();
                            buyer.inventoryMenu.broadcastChanges();
                            return;
                        }
                        sendAllDataToRemote();
                    }
                };
            }, net.minecraft.network.chat.Component.literal("Controlled fixture shop")));
            return net.minecraft.world.InteractionResult.SUCCESS_SERVER;
        }
    }
}
