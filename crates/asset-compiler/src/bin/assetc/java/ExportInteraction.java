// Original offline exporter. No JVM or vanilla client runs inside Cinnabar.
import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.shapes.VoxelShape;

public final class ExportInteraction {
    private static void shape(DataOutputStream out, VoxelShape shape) throws Exception {
        var boxes = shape.toAabbs();
        out.writeInt(boxes.size());
        for (AABB box : boxes) {
            out.writeDouble(box.minX); out.writeDouble(box.minY); out.writeDouble(box.minZ);
            out.writeDouble(box.maxX); out.writeDouble(box.maxY); out.writeDouble(box.maxZ);
        }
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 1) throw new IllegalArgumentException("expected interaction output path");
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        try (var out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(Path.of(args[0]))))) {
            out.writeBytes("CBJHITS1");
            out.writeInt(Block.BLOCK_STATE_REGISTRY.size());
            for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
                out.writeInt(Block.getId(state));
                // Position/entity-dependent shapes need a native context implementation.
                boolean contextual = state.getBlock().hasDynamicShape() || state.hasOffsetFunction();
                out.writeBoolean(contextual);
                out.writeFloat(state.getDestroySpeed(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
                out.writeBoolean(state.requiresCorrectToolForDrops());
                if (contextual) {
                    out.writeInt(0); out.writeInt(0);
                } else {
                    shape(out, state.getShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
                    shape(out, state.getInteractionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO));
                }
            }
        }
    }
}
