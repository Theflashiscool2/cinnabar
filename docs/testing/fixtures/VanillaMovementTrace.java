/** Original opt-in movement sampling and native keepalive probe for verified vanilla26.3. */
public final class VanillaMovementTrace {
    private static final java.util.concurrent.ScheduledExecutorService TIMER =
        java.util.concurrent.Executors.newSingleThreadScheduledExecutor(r -> {
            var thread = new Thread(r, "acceptance-movement-trace");
            thread.setDaemon(true);
            return thread;
        });
    private static final java.util.concurrent.atomic.AtomicInteger NEXT = new java.util.concurrent.atomic.AtomicInteger();
    private static Runnable stop;

    public static void register(net.minecraft.server.MinecraftServer server, java.nio.file.Path root) {
        server.getCommands().getDispatcher().register(net.minecraft.commands.Commands.literal("acceptance_keepalive")
            .requires(source -> source.getEntity() == null)
            .then(net.minecraft.commands.Commands.argument("player", com.mojang.brigadier.arguments.StringArgumentType.word())
                .executes(context -> {
                    var player = server.getPlayerList().getPlayerByName(com.mojang.brigadier.arguments.StringArgumentType.getString(context, "player"));
                    if (player == null) throw new IllegalArgumentException("Player must be connected");
                    try {
                        var common = net.minecraft.server.network.ServerCommonPacketListenerImpl.class;
                        var pending = common.getDeclaredField("keepAlivePending");
                        pending.setAccessible(true);
                        if (pending.getBoolean(player.connection)) throw new IllegalStateException("A native challenge is already pending");
                        var time = common.getDeclaredField("keepAliveTime");
                        time.setAccessible(true);
                        // Make one native check due earlier; the native challenge, validation and
                        // response timeout remain unchanged. This is a probe, never a fix.
                        time.setLong(player.connection, net.minecraft.util.Util.getMillis()
                            - net.minecraft.server.network.ServerCommonPacketListenerImpl.LATENCY_CHECK_INTERVAL);
                        var check = common.getDeclaredMethod("keepConnectionAlive");
                        check.setAccessible(true);
                        check.invoke(player.connection);
                        System.out.println("CINNABAR early native keepalive probe; unix_ms=" + System.currentTimeMillis());
                    } catch (ReflectiveOperationException error) { throw new IllegalStateException(error); }
                    return 1;
                })));
        server.getCommands().getDispatcher().register(net.minecraft.commands.Commands.literal("acceptance_trace")
            .requires(source -> source.getEntity() == null)
            .then(net.minecraft.commands.Commands.argument("player", com.mojang.brigadier.arguments.StringArgumentType.word())
                .executes(context -> {
                    try { start(server, root, com.mojang.brigadier.arguments.StringArgumentType.getString(context, "player")); }
                    catch (Exception error) { throw new IllegalStateException("Cannot start acceptance trace", error); }
                    return 1;
                })));
    }

    private static java.lang.reflect.Field field(String name) throws Exception {
        var field = net.minecraft.server.network.ServerGamePacketListenerImpl.class.getDeclaredField(name);
        field.setAccessible(true);
        return field;
    }

    private static void start(net.minecraft.server.MinecraftServer server, java.nio.file.Path root, String name) throws Exception {
        if (stop != null) stop.run();
        java.nio.file.Files.createDirectories(root);
        var path = root.resolve("movement-" + System.currentTimeMillis() + "-" + NEXT.incrementAndGet() + ".jsonl");
        var writer = java.nio.file.Files.newBufferedWriter(path, java.nio.charset.StandardCharsets.UTF_8,
            java.nio.file.StandardOpenOption.CREATE_NEW);
        var enabled = new java.util.concurrent.atomic.AtomicBoolean(true);
        var pending = new java.util.concurrent.atomic.AtomicBoolean();
        var firstPlayer = new java.util.concurrent.atomic.AtomicLong();
        long started = System.nanoTime();
        var floating = field("clientIsFloating");
        var above = field("aboveGroundTickCount");
        var awaiting = field("awaitingPositionFromClient");
        var teleport = field("awaitingTeleport");
        var load = field("clientLoadedTimeoutTimer");
        var gson = new com.google.gson.Gson();
        var task = new java.util.concurrent.atomic.AtomicReference<java.util.concurrent.ScheduledFuture<?>>();
        Runnable close = () -> {
            enabled.set(false);
            if (task.get() != null) task.get().cancel(false);
            try { writer.close(); } catch (Exception error) { error.printStackTrace(); }
        };
        stop = close;
        task.set(TIMER.scheduleAtFixedRate(() -> {
            if (!enabled.get()) return;
            // At most one sample is queued; a busy server never accumulates diagnostic work.
            if (!pending.compareAndSet(false, true)) return;
            server.execute(() -> {
                try {
                    long now = System.nanoTime();
                    var player = server.getPlayerList().getPlayerByName(name);
                    if (!enabled.get() || now - started > 120_000_000_000L
                            || (firstPlayer.get() != 0 && (player == null || now - firstPlayer.get() > 60_000_000_000L))) {
                        close.run();
                        return;
                    }
                    if (player == null) return;
                    firstPlayer.compareAndSet(0, now);
                    var row = new com.google.gson.JsonObject();
                    var wall = java.time.Instant.now();
                    row.addProperty("unix_ns", wall.getEpochSecond() * 1_000_000_000L + wall.getNano());
                    row.addProperty("monotonic_ns", now);
                    row.addProperty("server_tick", server.getTickCount());
                    row.addProperty("entity", player.getId());
                    var pos = player.position();
                    var motion = player.getDeltaMovement();
                    row.add("position", gson.toJsonTree(new double[]{pos.x, pos.y, pos.z}));
                    row.add("velocity", gson.toJsonTree(new double[]{motion.x, motion.y, motion.z}));
                    row.addProperty("on_ground", player.onGround());
                    row.addProperty("client_loaded", player.connection.hasClientLoaded());
                    row.addProperty("load_timer", load.getInt(player.connection));
                    row.addProperty("client_floating", floating.getBoolean(player.connection));
                    row.addProperty("floating_ticks", above.getInt(player.connection));
                    row.addProperty("awaiting_teleport", awaiting.get(player.connection) != null);
                    row.addProperty("teleport_id", teleport.getInt(player.connection));
                    row.addProperty("block_below", player.level().getBlockState(player.blockPosition().below()).toString());
                    row.addProperty("collision_below", !player.level().noCollision(player, player.getBoundingBox().move(0, -0.0625, 0)));
                    writer.write(gson.toJson(row));
                    writer.newLine();
                    writer.flush();
                } catch (Exception error) {
                    close.run();
                    error.printStackTrace();
                } finally {
                    pending.set(false);
                }
            });
        }, 0, 100, java.util.concurrent.TimeUnit.MILLISECONDS));
        System.out.println("CINNABAR movement trace: " + path);
    }
}
