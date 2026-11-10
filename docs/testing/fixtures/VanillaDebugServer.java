/** Acceptance-only bootstrap: enable vanilla's own debugconfig administrative command.
 * Uses verified unmodified26.3 server/classes. Optional console-only commands exercise
 * official player-list packets against real connected players; they do not fabricate players.
 */
public final class VanillaDebugServer {
    public static void main(String[] args) throws Exception {
        net.minecraft.SharedConstants.IS_RUNNING_IN_IDE = true;
        net.minecraft.server.Main.main(args);
        if (Boolean.getBoolean("cinnabar.acceptance.entityCommands")
                || Boolean.getBoolean("cinnabar.acceptance.shop")) {
            var server = findServer();
            server.execute(() -> {
                if (Boolean.getBoolean("cinnabar.acceptance.shop")) VanillaShopFixture.register(server);
                register(server, "acceptance_unlist", false);
                register(server, "acceptance_relist", true);
                var traceRoot = System.getProperty("cinnabar.acceptance.trace");
                if (traceRoot != null) VanillaMovementTrace.register(server, java.nio.file.Path.of(traceRoot));
                System.out.println("CINNABAR acceptance entity commands ready");
            });
        }
    }

    // Pinned JDK25's platform Thread task captures MinecraftServer.spin's AtomicReference.
    // Opt-in fixture only: --add-opens java.base/java.lang=ALL-UNNAMED is required.
    private static net.minecraft.server.MinecraftServer findServer() throws Exception {
        var holder = Thread.class.getDeclaredField("holder");
        holder.setAccessible(true);
        for (var thread : Thread.getAllStackTraces().keySet()) {
            if (!thread.getName().equals("Server thread")) continue;
            var value = holder.get(thread);
            var taskField = value.getClass().getDeclaredField("task");
            taskField.setAccessible(true);
            var task = taskField.get(value);
            for (var capture : task.getClass().getDeclaredFields()) {
                capture.setAccessible(true);
                if (capture.get(task) instanceof java.util.concurrent.atomic.AtomicReference<?> ref
                        && ref.get() instanceof net.minecraft.server.MinecraftServer server) return server;
            }
        }
        throw new IllegalStateException("Pinned vanilla server thread/reference unavailable");
    }

    private static void register(net.minecraft.server.MinecraftServer server, String name, boolean add) {
        server.getCommands().getDispatcher().register(net.minecraft.commands.Commands.literal(name)
            .requires(source -> source.getEntity() == null)
            .then(net.minecraft.commands.Commands.argument("target", com.mojang.brigadier.arguments.StringArgumentType.word())
                .then(net.minecraft.commands.Commands.argument("viewer", com.mojang.brigadier.arguments.StringArgumentType.word())
                    .executes(context -> {
                        var target = server.getPlayerList().getPlayerByName(com.mojang.brigadier.arguments.StringArgumentType.getString(context, "target"));
                        var viewer = server.getPlayerList().getPlayerByName(com.mojang.brigadier.arguments.StringArgumentType.getString(context, "viewer"));
                        if (target == null || viewer == null) throw new IllegalArgumentException("Both players must be connected");
                        if (add) viewer.connection.send(net.minecraft.network.protocol.game.ClientboundPlayerInfoUpdatePacket.createPlayerInitializing(java.util.List.of(target)));
                        else viewer.connection.send(new net.minecraft.network.protocol.game.ClientboundPlayerInfoRemovePacket(java.util.List.of(target.getUUID())));
                        System.out.println(name + " sent; target=" + target.getUUID() + " viewer=" + viewer.getUUID());
                        return 1;
                    }))));
    }
}
