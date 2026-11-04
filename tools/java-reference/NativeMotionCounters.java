// Original read-only fixture: observes real packet counters on the opted-in isolated server.
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Field;
import java.nio.file.*;
import java.util.List;
import java.util.function.Consumer;

public final class NativeMotionCounters {
    private static Path request;
    private static Consumer<Object> previous, callback;
    private static Field receiver;
    private static int ticks;
    public static void premain(String path, Instrumentation unused) throws Exception { install(path); }
    public static void agentmain(String path, Instrumentation unused) throws Exception { install(path); }
    @SuppressWarnings("unchecked")
    private static synchronized void install(String path) throws Exception {
        if (callback != null) throw new IllegalStateException("Motion counter fixture already installed");
        request=Path.of(path);
        if (!request.isAbsolute()) throw new IllegalArgumentException("Absolute request path required");
        receiver=Class.forName("NativeMenuFixtureBridge",true,null).getField("receiver");
        previous=(Consumer<Object>)receiver.get(null);
        if (previous==null) throw new IllegalStateException("Opt-in NativeMenuFixture required first");
        callback=server -> { previous.accept(server); sample(server); };
        receiver.set(null,callback);
        System.out.println("NATIVE_MOTION_FIXTURE installed; read-only, no packet or world replacement");
    }
    private static void sample(Object server) {
        if (++ticks%20!=0 || !Files.isRegularFile(request)) return;
        try {
            if (Files.size(request)>64) { Files.delete(request); throw new IllegalArgumentException("Request too large"); }
            String command=Files.readString(request).trim();Files.delete(request);
            if (command.equals("detach")) {
                if (receiver.get(null)==callback) receiver.set(null,previous);
                System.out.println("NATIVE_MOTION_FIXTURE detached"); return;
            }
            if (!command.equals("sample")) throw new IllegalArgumentException("Use sample or detach");
            Object list=server.getClass().getMethod("getPlayerList").invoke(server);
            List<?> players=(List<?>)list.getClass().getMethod("getPlayers").invoke(list);
            if (players.size()>32) throw new IllegalStateException("Isolated fixture server required");
            for (Object player:players) {
                Class<?> type=player.getClass();Object connection=type.getField("connection").get(player);
                var values=new StringBuilder();
                for (String name:new String[]{"tickCount","receivedMovePacketCount","knownMovePacketCount"}) {
                    Field value=connection.getClass().getDeclaredField(name);value.setAccessible(true);
                    values.append(" ").append(name).append("=").append(value.getInt(connection));
                }
                System.out.println("NATIVE_MOTION_FIXTURE name="+type.getMethod("getScoreboardName").invoke(player)
                    +values+" on_ground="+type.getMethod("onGround").invoke(player)
                    +" bounds="+type.getMethod("getBoundingBox").invoke(player));
            }
        } catch (Throwable error) { System.err.println("NATIVE_MOTION_FIXTURE failed: "+error); }
    }
    public static void main(String[] args) throws Exception {
        if (args.length!=3) throw new IllegalArgumentException("Usage: PID agent.jar absolute-request-path");
        var vm=com.sun.tools.attach.VirtualMachine.attach(args[0]);
        try { vm.loadAgent(args[1],args[2]); } finally { vm.detach(); }
    }
}
