// Bootstrap-visible callback: the vanilla server's isolated class loader cannot see agent classes.
public final class NativeMenuFixtureBridge {
    public static volatile java.util.function.Consumer<Object> receiver;
    public static void tick(Object server) {
        var callback = receiver;
        if (callback != null) callback.accept(server);
    }
}
