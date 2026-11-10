// Original offline prediction-facts exporter for the verified protocol 777 runtime.
// Records declaring classes and actual FluidState.createLegacyBlock IDs, not model guesses.
import com.google.gson.*;
import java.lang.reflect.Method;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HexFormat;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.Item;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.GameMasterBlock;
import net.minecraft.world.level.block.state.BlockBehaviour;
public final class ExportPrediction {
    static String owner(Class<?> type, Class<?> base, String name) throws Exception {
        Method target = null;
        for (var method : base.getDeclaredMethods()) {
            if (!method.getName().equals(name)) continue;
            if (target != null) throw new IllegalStateException("overloaded " + name);
            target = method;
        }
        if (target == null) throw new NoSuchMethodException(name);
        for (var current = type; current != null; current = current.getSuperclass()) {
            try { return current.getDeclaredMethod(name, target.getParameterTypes()).getDeclaringClass().getName(); }
            catch (NoSuchMethodException ignored) {}
        }
        throw new NoSuchMethodException(type.getName() + "." + name);
    }
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("output.json exporter.java");
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var root = new JsonObject();
        root.addProperty("schema", 1);
        root.addProperty("version", SharedConstants.getCurrentVersion().id());
        root.addProperty("protocol", SharedConstants.getCurrentVersion().protocolVersion());
        root.addProperty("source_sha256", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[1])))));
        var blocks = new JsonArray();
        for (var block : BuiltInRegistries.BLOCK) {
            var row = new JsonObject();
            row.addProperty("name", BuiltInRegistries.BLOCK.getKey(block).toString());
            row.addProperty("game_master", block instanceof GameMasterBlock);
            var callbacks = new JsonObject();
            for (var name : new String[]{"onPlace", "updateShape", "updateIndirectNeighbourShapes", "neighborChanged"})
                callbacks.addProperty(name, owner(block.getClass(), BlockBehaviour.class, name));
            for (var name : new String[]{"setPlacedBy", "playerWillDestroy", "destroy"})
                callbacks.addProperty(name, owner(block.getClass(), Block.class, name));
            row.add("callbacks", callbacks); blocks.add(row);
        }
        root.add("blocks", blocks);
        var items = new JsonArray();
        for (var item : BuiltInRegistries.ITEM) {
            var row = new JsonArray(); row.add(BuiltInRegistries.ITEM.getKey(item).toString());
            row.add(owner(item.getClass(), Item.class, "canDestroyBlock")); items.add(row);
        }
        root.add("items", items);
        var runs = new JsonArray();
        int start = 0, count = 0, previousBlock = -1, previousLegacy = -1;
        for (var state : Block.BLOCK_STATE_REGISTRY) {
            int id = Block.BLOCK_STATE_REGISTRY.getId(state);
            int block = BuiltInRegistries.BLOCK.getId(state.getBlock());
            int legacy = Block.BLOCK_STATE_REGISTRY.getId(state.getFluidState().createLegacyBlock());
            if (legacy < 0) throw new IllegalStateException("unregistered fluid legacy state");
            if (count > 0 && (block != previousBlock || legacy != previousLegacy)) {
                var row = new JsonArray(); row.add(start); row.add(count); row.add(previousBlock); row.add(previousLegacy); runs.add(row);
                start = id; count = 0;
            }
            previousBlock = block; previousLegacy = legacy; count++;
        }
        if (count > 0) {
            var row = new JsonArray(); row.add(start); row.add(count); row.add(previousBlock); row.add(previousLegacy); runs.add(row);
        }
        root.addProperty("state_count", start + count); root.add("runs", runs);
        Files.writeString(Path.of(args[0]), new GsonBuilder().disableHtmlEscaping().create().toJson(root) + "\n", StandardCharsets.UTF_8);
    }
}
