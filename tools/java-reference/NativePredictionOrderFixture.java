// Standalone FastUtil-only oracle for the map used by Java 26.3 block prediction.
// Run with the version-pinned fastutil-8.5.18.jar; no Minecraft server is started.
import it.unimi.dsi.fastutil.longs.Long2ObjectOpenHashMap;
public class NativePredictionOrderFixture {
    static long key(int id) {
        if (id == 0) return 0;
        int x = (id * 17) % 61 - 30, y = -64 + id % 16, z = (id * 29) % 73 - 36;
        return ((long)x & 0x3ffffffL) << 38 | ((long)z & 0x3ffffffL) << 12 | (y & 0xfffL);
    }
    static String order(Long2ObjectOpenHashMap<Integer> map) {
        var result = new StringBuilder("[");
        for (int id : map.values()) {
            if (result.length() > 1) result.append(',');
            result.append(id);
        }
        return result.append(']').toString();
    }
    public static void main(String[] args) {
        var map = new Long2ObjectOpenHashMap<Integer>();
        for (int id=0; id<80; id++) map.put(key(id), Integer.valueOf(id));
        System.out.print("{\"before\":" + order(map));
        var removed = new StringBuilder("[");
        var iterator = map.long2ObjectEntrySet().iterator();
        while (iterator.hasNext()) {
            int id = iterator.next().getValue();
            if (id % 3 == 0) {
                if (removed.length() > 1) removed.append(',');
                removed.append(id);
                iterator.remove();
            }
        }
        System.out.print(",\"removed\":" + removed.append(']') + ",\"after\":" + order(map));
        for (int id=80; id<110; id++) map.put(key(id), Integer.valueOf(id));
        System.out.println(",\"extended\":" + order(map) + "}");
    }
}
