// Original offline native fluid-current contact facts. No JVM is used by Cinnabar.
import com.google.gson.*;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.*;

public final class ExportFluidContactFacts {
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("Usage: OUTPUT BLOCKS_JSON");
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var version = JsonParser.parseReader(new InputStreamReader(
            ExportFluidContactFacts.class.getResourceAsStream("/version.json"), StandardCharsets.UTF_8)).getAsJsonObject();
        var out = new JsonObject(); out.addProperty("schema", 1);
        out.add("game_version", version.get("id")); out.add("protocol", version.get("protocol_version"));
        out.addProperty("fast_lava_default", net.minecraft.world.attribute.EnvironmentAttributes.FAST_LAVA.defaultValue());
        out.addProperty("blocks_sha256", java.util.HexFormat.of().formatHex(
            java.security.MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[1])))));
        // Native FlowingFluid.isSolidFace uses this direction, not its opposite.
        Direction[] faces = {Direction.WEST, Direction.EAST, Direction.DOWN,
            Direction.UP, Direction.NORTH, Direction.SOUTH};
        var runs = new JsonArray(); int previous = -1, count = 0, states = 0;
        for (var state : Block.BLOCK_STATE_REGISTRY) {
            if (Block.getId(state) != states++) throw new IllegalStateException("Registry order");
            int mask = state.getBlock() instanceof IceBlock ? 128 : 0;
            for (int i = 0; i < faces.length; i++)
                if (state.isFaceSturdy(EmptyBlockGetter.INSTANCE, BlockPos.ZERO, faces[i])) mask |= 1 << i;
            if (mask != previous && count != 0) {
                var run = new JsonArray(); run.add(count); run.add(previous); runs.add(run); count = 0;
            }
            previous = mask; count++;
        }
        var run = new JsonArray(); run.add(count); run.add(previous); runs.add(run);
        out.addProperty("states", states); out.add("runs", runs);
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(out) + "\n");
    }
}
