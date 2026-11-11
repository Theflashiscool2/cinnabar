/** Console-only acceptance commands using the real ServerPlayer abilities owner and packet. */
public final class VanillaFlightFixture {
    public static void register(net.minecraft.server.MinecraftServer server) {
        var dispatcher = server.getCommands().getDispatcher();
        dispatcher.register(net.minecraft.commands.Commands.literal("acceptance_flight")
                .requires(source -> source.getEntity() == null)
                .then(net.minecraft.commands.Commands.argument("player", net.minecraft.commands.arguments.EntityArgument.player())
                        .then(net.minecraft.commands.Commands.argument("enabled", com.mojang.brigadier.arguments.BoolArgumentType.bool())
                                .executes(context -> {
                                    var player = net.minecraft.commands.arguments.EntityArgument.getPlayer(context, "player");
                                    boolean enabled = com.mojang.brigadier.arguments.BoolArgumentType.getBool(context, "enabled");
                                    player.getAbilities().mayfly = enabled;
                                    player.getAbilities().flying = enabled;
                                    player.onUpdateAbilities();
                                    status(player);
                                    return 1;
                                }))));
        dispatcher.register(net.minecraft.commands.Commands.literal("acceptance_flight_status")
                .requires(source -> source.getEntity() == null)
                .then(net.minecraft.commands.Commands.argument("player", net.minecraft.commands.arguments.EntityArgument.player())
                        .executes(context -> {
                            status(net.minecraft.commands.arguments.EntityArgument.getPlayer(context, "player"));
                            return 1;
                        })));
    }
    private static void status(net.minecraft.server.level.ServerPlayer player) {
        var abilities = player.getAbilities();
        System.out.println("CINNABAR flight mayfly=" + abilities.mayfly + " flying=" + abilities.flying
                + " speed=" + abilities.getFlyingSpeed() + " on_ground=" + player.onGround());
    }
}
