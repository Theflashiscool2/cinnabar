// Original opt-in isolated-server fixture. Activates the real ServerItemCooldowns API.
import java.lang.instrument.Instrumentation;
import java.lang.reflect.Field;
import java.nio.file.*;
import java.util.function.Consumer;

public final class NativeCooldownFixture {
    private static Path request;
    private static String name;
    private static Field receiver;
    private static Consumer<Object> previous, callback;
    private static int ticks;
    @SuppressWarnings("unchecked")
    public static void agentmain(String argument, Instrumentation unused) throws Exception {
        if (callback != null) throw new IllegalStateException("Already installed");
        var parts=argument.split(",",2);request=Path.of(parts[0]);name=parts[1];
        if (!request.isAbsolute()) throw new IllegalArgumentException("Absolute dedicated request required");
        receiver=Class.forName("NativeMenuFixtureBridge",true,null).getField("receiver");
        previous=(Consumer<Object>)receiver.get(null);
        if (previous==null) throw new IllegalStateException("Install the isolated menu fixture first");
        callback=server -> {previous.accept(server);poll(server);};receiver.set(null,callback);
        System.out.println("NATIVE_COOLDOWN_FIXTURE installed, real vanilla cooldown packets only");
    }
    private static void poll(Object server) {
        if (++ticks%20!=0 || !Files.isRegularFile(request)) return;
        try {
            if (Files.size(request)>128) {Files.delete(request);throw new IllegalArgumentException("Oversized request");}
            var command=Files.readString(request).trim();Files.delete(request);
            if (command.equals("detach")) {
                if (receiver.get(null)==callback) receiver.set(null,previous);
                System.out.println("NATIVE_COOLDOWN_FIXTURE detached");return;
            }
            Object list=server.getClass().getMethod("getPlayerList").invoke(server);
            Object player=list.getClass().getMethod("getPlayerByName",String.class).invoke(list,name);
            if (player==null) throw new IllegalStateException("Fixture player not connected");
            Object cooldowns=player.getClass().getMethod("getCooldowns").invoke(player);
            Object stack=player.getClass().getMethod("getMainHandItem").invoke(player);
            Class<?> stackType=stack.getClass();
            Object group=cooldowns.getClass().getMethod("getCooldownGroup",stackType).invoke(cooldowns,stack);
            if (command.startsWith("add ")) {
                int duration=Integer.parseInt(command.substring(4));
                if (duration<1 || duration>2400) throw new IllegalArgumentException("Use 1..2400 ticks");
                cooldowns.getClass().getMethod("addCooldown",stackType,int.class).invoke(cooldowns,stack,duration);
            } else if (command.equals("remove")) {
                cooldowns.getClass().getMethod("removeCooldown",group.getClass()).invoke(cooldowns,group);
            } else if (!command.equals("status")) throw new IllegalArgumentException("Use add N, remove, status, detach");
            System.out.println("NATIVE_COOLDOWN_FIXTURE command="+command+" player="+name+" group="+group
                +" active="+cooldowns.getClass().getMethod("isOnCooldown",stackType).invoke(cooldowns,stack));
        } catch (Throwable error) {System.err.println("NATIVE_COOLDOWN_FIXTURE failed: "+error);}
    }
    public static void main(String[] args) throws Exception {
        if (args.length!=3) throw new IllegalArgumentException("Usage: PID agent.jar absolute-request,player");
        var vm=com.sun.tools.attach.VirtualMachine.attach(args[0]);
        try {vm.loadAgent(args[1],args[2]);}finally {vm.detach();}
    }
}
