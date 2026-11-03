// Opt-in controlled-server admin fixture. No client state, packets, or transaction results are faked.
// JDK 25 Class-File API; target vanilla Java 26.3 ChestMenu and SimpleMenuProvider APIs.
import java.lang.classfile.*;
import java.lang.classfile.instruction.ReturnInstruction;
import java.lang.constant.*;
import java.lang.instrument.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.security.ProtectionDomain;
import java.util.*;

public final class NativeMenuFixture {
    private static Path request;
    private static String playerName;
    private static Object current;
    private static int ticks;
    public static void premain(String args, Instrumentation inst) throws Exception { install(args, inst); }
    public static void agentmain(String args, Instrumentation inst) throws Exception { install(args, inst); }
    private static void install(String args, Instrumentation inst) throws Exception {
        String[] options = args.split(",", -1);
        if (options.length != 2 || !options[1].matches("[A-Za-z0-9_]{1,16}"))
            throw new IllegalArgumentException("Expected absolute-request-path,fixture-player-name");
        request = Path.of(options[0]);
        if (!request.isAbsolute()) throw new IllegalArgumentException("Request path must be absolute");
        playerName = options[1];
        Path bridge = Path.of(NativeMenuFixture.class.getProtectionDomain().getCodeSource().getLocation().toURI())
            .resolveSibling("fixture-bridge.jar");
        inst.appendToBootstrapClassLoaderSearch(new java.util.jar.JarFile(bridge.toFile()));
        Class.forName("NativeMenuFixtureBridge", true, null).getField("receiver")
            .set(null, (java.util.function.Consumer<Object>)NativeMenuFixture::tick);
        inst.addTransformer(new ClassFileTransformer() {
            public byte[] transform(ClassLoader loader, String name, Class<?> type,
                                    ProtectionDomain domain, byte[] bytes) {
                if (!name.equals("net/minecraft/server/MinecraftServer")) return null;
                var cf = ClassFile.of();
                return cf.transformClass(cf.parse(bytes), ClassTransform.transformingMethodBodies(
                    method -> method.methodName().equalsString("tickServer"), (builder, element) -> {
                        if (element instanceof ReturnInstruction) builder.aload(0).invokestatic(
                            ClassDesc.of("NativeMenuFixtureBridge"), "tick",
                            MethodTypeDesc.of(ConstantDescs.CD_void, ConstantDescs.CD_Object));
                        builder.with(element);
                    }));
            }
        }, true);
        for (Class<?> type : inst.getAllLoadedClasses())
            if (type.getName().equals("net.minecraft.server.MinecraftServer")) inst.retransformClasses(type);
        System.out.println("NATIVE_MENU_FIXTURE armed for " + playerName + "; vanilla still owns all menu actions");
    }
    // Runs on the real server tick, with bounded polling and a tiny command vocabulary.
    public static void tick(Object server) {
        if (++ticks % 20 != 0 || !Files.isRegularFile(request)) return;
        try {
            if (Files.size(request) > 32) throw new IllegalArgumentException("Oversized fixture request");
            String command = Files.readString(request).trim();
            Files.delete(request);
            ClassLoader loader = server.getClass().getClassLoader();
            Class<?> serverType = loader.loadClass("net.minecraft.server.MinecraftServer");
            Object list = serverType.getMethod("getPlayerList").invoke(server);
            Object player = list.getClass().getMethod("getPlayerByName", String.class).invoke(list, playerName);
            if (player == null) throw new IllegalStateException("Fixture player is not connected");
            Class<?> chest = loader.loadClass("net.minecraft.world.inventory.ChestMenu");
            Class<?> container = loader.loadClass("net.minecraft.world.Container");
            Class<?> stack = loader.loadClass("net.minecraft.world.item.ItemStack");
            if (command.matches("open [1-6]")) {
                int rows = Integer.parseInt(command.substring(5));
                Class<?> constructor = loader.loadClass("net.minecraft.world.inventory.MenuConstructor");
                Object factory = Proxy.newProxyInstance(loader, new Class<?>[]{constructor}, (proxy, method, values) -> {
                    if (!method.getName().equals("createMenu")) throw new UnsupportedOperationException(method.getName());
                    String[] methods = {"oneRow", "twoRows", "threeRows", "fourRows", "fiveRows", "sixRows"};
                    Object menu = chest.getMethod(methods[rows-1], int.class,
                        loader.loadClass("net.minecraft.world.entity.player.Inventory")).invoke(null, values[0], values[1]);
                    current = menu;
                    Object contents = chest.getMethod("getContainer").invoke(menu);
                    Class<?> itemLike = loader.loadClass("net.minecraft.world.level.ItemLike");
                    Class<?> items = loader.loadClass("net.minecraft.world.item.Items");
                    Object stone = stack.getConstructor(itemLike, int.class).newInstance(items.getField("STONE").get(null), 11);
                    Object gold = stack.getConstructor(itemLike, int.class).newInstance(items.getField("GOLD_INGOT").get(null), 9);
                    container.getMethod("setItem", int.class, stack).invoke(contents, 0, stone);
                    container.getMethod("setItem", int.class, stack).invoke(contents, rows*9-1, gold);
                    return menu;
                });
                Class<?> component = loader.loadClass("net.minecraft.network.chat.Component");
                Object title = component.getMethod("literal", String.class).invoke(null, "Native " + rows + " rows");
                Object provider = loader.loadClass("net.minecraft.world.SimpleMenuProvider")
                    .getConstructor(constructor, component).newInstance(factory, title);
                player.getClass().getMethod("openMenu", loader.loadClass("net.minecraft.world.MenuProvider")).invoke(player, provider);
                System.out.println("NATIVE_MENU_FIXTURE opened rows=" + rows);
            } else if (command.equals("players")) {
                List<?> players = (List<?>)list.getClass().getMethod("getPlayers").invoke(list);
                if (players.size() > 32) throw new IllegalStateException("Use an isolated fixture server");
                for (Object tracked : players) {
                    Class<?> type = tracked.getClass();
                    System.out.println("NATIVE_PLAYER_FIXTURE name=" + type.getMethod("getScoreboardName").invoke(tracked)
                        + " pose=" + type.getMethod("getPose").invoke(tracked)
                        + " bounds=" + type.getMethod("getBoundingBox").invoke(tracked)
                        + " health=" + type.getMethod("getHealth").invoke(tracked));
                }
            } else if (command.equals("dump") && current != null) {
                Object contents = chest.getMethod("getContainer").invoke(current);
                int size = (Integer)container.getMethod("getContainerSize").invoke(contents);
                List<String> slots = new ArrayList<>();
                for (int slot=0; slot<size; slot++) {
                    Object item = container.getMethod("getItem", int.class).invoke(contents, slot);
                    if (!(Boolean)stack.getMethod("isEmpty").invoke(item)) slots.add(slot + "=" + item);
                }
                Object carried = current.getClass().getMethod("getCarried").invoke(current);
                System.out.println("NATIVE_MENU_FIXTURE authority slots=" + slots + " carried=" + carried
                    + " all_menu_slots=" + current.getClass().getMethod("getItems").invoke(current));
            } else throw new IllegalArgumentException("Use open 1..6, dump, or players");
        } catch (Throwable error) {
            System.err.println("NATIVE_MENU_FIXTURE failed: " + error);
            error.printStackTrace();
        }
    }
    public static void main(String[] args) throws Exception {
        if (args.length != 3) throw new IllegalArgumentException("Usage: PID agent.jar absolute-request-path,fixture-player");
        var vm = com.sun.tools.attach.VirtualMachine.attach(args[0]);
        try { vm.loadAgent(args[1], args[2]); } finally { vm.detach(); }
    }
}
