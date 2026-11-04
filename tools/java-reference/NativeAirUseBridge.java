// Bootstrap-only bridge for the explicitly enabled isolated-server acceptance fixture.
public final class NativeAirUseBridge {
    public static volatile java.util.function.BiConsumer<Object,Object> receiver;
    public static void accept(Object player,Object packet) {
        var current=receiver;if(current!=null)current.accept(player,packet);
    }
}
