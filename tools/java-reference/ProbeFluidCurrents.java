// Original independent calls into the verified native FlowingFluid implementation.
import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.BlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.material.*;

public final class ProbeFluidCurrents implements BlockGetter {
    final Map<BlockPos, BlockState> blocks = new HashMap<>();
    public BlockState getBlockState(BlockPos p) { return blocks.getOrDefault(p, Blocks.AIR.defaultBlockState()); }
    public FluidState getFluidState(BlockPos p) { return getBlockState(p).getFluidState(); }
    public BlockEntity getBlockEntity(BlockPos p) { return null; }
    public int getHeight() { return 32; }
    public int getMinY() { return -16; }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        JsonArray result = new JsonArray();
        for (String name : List.of("isolated", "east_lower", "east_drop", "falling_wall", "falling_ice",
                "falling_glass", "falling_slab", "falling_leaves", "lava_east", "unequal_diagonal")) {
            var world = new ProbeFluidCurrents();
            var origin = new BlockPos(8, 0, 8);
            var block = name.equals("lava_east") ? Blocks.LAVA : Blocks.WATER;
            world.blocks.put(origin, block.defaultBlockState().setValue(LiquidBlock.LEVEL, name.startsWith("falling") ? 8 : 0));
            if (name.equals("east_lower") || name.equals("lava_east") || name.equals("unequal_diagonal"))
                world.blocks.put(origin.east(), block.defaultBlockState().setValue(LiquidBlock.LEVEL, 1));
            if (name.equals("unequal_diagonal"))
                world.blocks.put(origin.south(), block.defaultBlockState().setValue(LiquidBlock.LEVEL, 5));
            if (name.equals("east_drop")) world.blocks.put(origin.east().below(), block.defaultBlockState());
            if (name.startsWith("falling")) world.blocks.put(origin.east(), switch (name) {
                case "falling_wall" -> Blocks.STONE.defaultBlockState();
                case "falling_glass" -> Blocks.GLASS.defaultBlockState();
                case "falling_slab" -> Blocks.OAK_SLAB.defaultBlockState();
                case "falling_leaves" -> Blocks.OAK_LEAVES.defaultBlockState();
                default -> Blocks.ICE.defaultBlockState();
            });
            var fluid = world.getFluidState(origin);
            var flow = fluid.getFlow(world, origin);
            var row = new JsonObject(); row.addProperty("name", name);
            var states = new JsonArray();
            for (var entry : world.blocks.entrySet()) {
                var cell = new JsonArray(); cell.add(entry.getKey().getX()); cell.add(entry.getKey().getY());
                cell.add(entry.getKey().getZ()); cell.add(Block.getId(entry.getValue())); states.add(cell);
            }
            row.add("blocks", states);
            var vector = new JsonArray(); vector.add(flow.x); vector.add(flow.y); vector.add(flow.z);
            row.add("flow", vector); result.add(row);
        }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
    }
}
