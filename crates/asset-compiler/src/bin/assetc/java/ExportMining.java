// Original offline factual-data exporter; compile only against the verified pinned archive.
// The native client never embeds or starts this JVM.
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.BlockState;

public final class ExportMining {
    public static void main(String[] args) throws Exception {
        if (args.length != 1) throw new IllegalArgumentException("expected mining report path");
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var version = SharedConstants.getCurrentVersion();
        var out = new StringBuilder("{\n  \"version\":\"").append(version.id())
            .append("\",\n  \"protocol\":").append(version.protocolVersion())
            .append(",\n  \"state_count\":").append(Block.BLOCK_STATE_REGISTRY.size())
            .append(",\n  \"block_count\":").append(BuiltInRegistries.BLOCK.size())
            .append(",\n  \"runs\":[\n");
        int start = 0, count = 0, block = -1, bits = 0;
        boolean correct = false, first = true;
        for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
            int id = Block.getId(state);
            int nextBlock = BuiltInRegistries.BLOCK.getId(state.getBlock());
            float hardness = state.getDestroySpeed(EmptyBlockGetter.INSTANCE, BlockPos.ZERO);
            if (!Float.isFinite(hardness)) throw new IllegalStateException("nonfinite hardness at " + id);
            int nextBits = Float.floatToRawIntBits(hardness);
            boolean nextCorrect = state.requiresCorrectToolForDrops();
            if (count != 0 && (nextBlock != block || nextBits != bits || nextCorrect != correct)) {
                if (!first) out.append(",\n");
                first = false;
                row(out, start, count, block, bits, correct);
                count = 0;
            }
            if (count == 0) { start = id; block = nextBlock; bits = nextBits; correct = nextCorrect; }
            if (id != start + count) throw new IllegalStateException("noncontiguous state IDs");
            count++;
        }
        if (count != 0) {
            if (!first) out.append(",\n");
            row(out, start, count, block, bits, correct);
        }
        out.append("\n  ]\n}\n");
        Files.writeString(Path.of(args[0]), out, StandardCharsets.UTF_8);
    }
    private static void row(StringBuilder out, int start, int count, int block, int bits, boolean correct) {
        out.append("    [").append(start).append(',').append(count).append(',').append(block)
            .append(',').append(Integer.toUnsignedString(bits)).append(',').append(correct).append(']');
    }
}
