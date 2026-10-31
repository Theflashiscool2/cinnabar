// Original offline exporter of state facts used by the native Java fluid renderer.
// Compile/run only against the verified pinned server. No JVM is used by the client.
import com.google.gson.*;
import java.io.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.Direction;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.*;
import net.minecraft.world.phys.shapes.*;

public final class ExportFluids {
    private static final Direction[] FACES = {Direction.WEST, Direction.EAST, Direction.DOWN,
        Direction.UP, Direction.NORTH, Direction.SOUTH};

    private static Set<String> tag(String name, Set<String> visiting) throws Exception {
        if (!visiting.add(name)) throw new IOException("recursive fluid-flow tag");
        String[] id = name.split(":", 2);
        String path = "/data/" + id[0] + "/tags/block/" + id[1] + ".json";
        Set<String> result = new TreeSet<>();
        try (InputStream stream = ExportFluids.class.getResourceAsStream(path)) {
            if (stream == null) throw new IOException("missing " + path);
            JsonArray values = JsonParser.parseReader(new InputStreamReader(stream, java.nio.charset.StandardCharsets.UTF_8))
                .getAsJsonObject().getAsJsonArray("values");
            for (JsonElement element : values) {
                String value = element.isJsonObject() ? element.getAsJsonObject().get("id").getAsString() : element.getAsString();
                if (value.startsWith("#")) result.addAll(tag(value.substring(1), visiting));
                else result.add(value);
            }
        }
        visiting.remove(name);
        return result;
    }

    // Maximum bottom-anchored fluid face fully covered by this neighbor. For upward
    // faces the fluid must reach the cell boundary; downward faces are at y=0.
    private static float coverage(BlockState state, Direction direction) {
        VoxelShape face = state.getFaceOcclusionShape(direction.getOpposite());
        if (face.isEmpty()) return 0;
        if (direction.getAxis() == Direction.Axis.Y) {
            return Shapes.blockOccludes(Shapes.block(), face, direction) ? 1 : 0;
        }
        Set<Double> heights = new TreeSet<>();
        heights.add(1.0);
        for (var box : face.toAabbs()) {
            if (box.maxY > 0 && box.maxY <= 1) heights.add(box.maxY);
        }
        float covered = 0;
        for (double height : heights) {
            if (Shapes.blockOccludes(Shapes.box(0,0,0,1,height,1), face, direction)) covered = (float)height;
        }
        return covered;
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 1) throw new IllegalArgumentException("expected fluid output path");
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Set<String> flowBlockers = tag("minecraft:blocks_fluid_flow", new HashSet<>());
        JsonObject root = new JsonObject();
        root.addProperty("schema", 1);
        JsonArray states = new JsonArray();
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            FluidState fluid = state.getFluidState();
            String kind = fluid.isEmpty() ? "empty" : fluid.getType().isSame(Fluids.WATER) ? "water"
                : fluid.getType().isSame(Fluids.LAVA) ? "lava" : "unsupported";
            if (kind.equals("unsupported")) throw new IOException("unknown fluid family");
            JsonObject entry = new JsonObject();
            entry.addProperty("id", Block.getId(state));
            entry.addProperty("kind", kind);
            entry.addProperty("height", fluid.getOwnHeight());
            entry.addProperty("falling", fluid.hasProperty(FlowingFluid.FALLING) && fluid.getValue(FlowingFluid.FALLING));
            entry.addProperty("solid", state.isSolid());
            entry.addProperty("solid_render", state.isSolidRender());
            entry.addProperty("flow_blocking", flowBlockers.contains(BuiltInRegistries.BLOCK.getKey(state.getBlock()).toString()));
            entry.addProperty("overlay", state.getBlock() instanceof HalfTransparentBlock || state.getBlock() instanceof LeavesBlock);
            entry.addProperty("invisible", state.getRenderShape() == RenderShape.INVISIBLE);
            entry.addProperty("emission", state.getLightEmission());
            JsonArray coverage = new JsonArray();
            for (Direction face : FACES) coverage.add(coverage(state, face));
            entry.add("occlusion", coverage);
            states.add(entry);
        }
        root.add("states", states);
        Files.writeString(Path.of(args[0]), new Gson().toJson(root) + "\n");
    }
}
