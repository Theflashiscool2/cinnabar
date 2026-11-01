// Original offline data exporter. Compile against the hash-verified vanilla server archive.
// This is asset preparation only; Cinnabar's native client never starts or embeds this JVM.
import java.io.BufferedOutputStream;
import java.io.DataOutputStream;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.util.List;
import java.util.Set;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.ai.attributes.RangedAttribute;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.EndGatewayBlock;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.phys.AABB;

public final class ExportPhysics {
    private static final Set<String> CALLBACKS = Set.of("stepOn", "fallOn", "bounceOn", "entityInside");

    // Context-dependent callbacks are explicit unsupported behavior until the Rust movement
    // owner implements them. An empty static shape is never used for a dynamic collision.
    private static boolean specialCallbacks(Block block) {
        for (Class<?> type = block.getClass(); type != Block.class; type = type.getSuperclass()) {
            for (var method : type.getDeclaredMethods()) {
                // 26.3 EndGatewayBlock.entityInside exits on the client. The server owns
                // eligibility, cooldown and teleport; local travel must enter its empty shape.
                if (type == EndGatewayBlock.class && method.getName().equals("entityInside")) continue;
                if (CALLBACKS.contains(method.getName())) return true;
            }
        }
        return false;
    }

    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("expected physics and player-attribute output paths");
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        try (var out = new DataOutputStream(new BufferedOutputStream(Files.newOutputStream(Path.of(args[0]))))) {
            out.writeBytes("CBJPHYS1");
            out.writeInt(Block.BLOCK_STATE_REGISTRY.size());
            for (BlockState state : Block.BLOCK_STATE_REGISTRY) {
                Block block = state.getBlock();
                boolean dynamic = block.hasDynamicShape();
                out.writeInt(Block.getId(state));
                out.writeByte((dynamic ? 1 : 0) | (specialCallbacks(block) ? 2 : 0));
                out.writeFloat(block.getFriction());
                out.writeFloat(block.getSpeedFactor());
                out.writeFloat(block.getJumpFactor());
                out.writeFloat(block.getBounceRestitution());
                out.writeBoolean(state.isAir());
                out.writeBoolean(state.getFluidState().isEmpty());
                byte[] fluid = BuiltInRegistries.FLUID.getKey(state.getFluidState().getType()).toString().getBytes(StandardCharsets.UTF_8);
                out.writeShort(fluid.length);
                out.write(fluid);
                List<AABB> boxes = dynamic ? List.of() : state.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).toAabbs();
                out.writeInt(boxes.size());
                for (AABB box : boxes) {
                    out.writeDouble(box.minX); out.writeDouble(box.minY); out.writeDouble(box.minZ);
                    out.writeDouble(box.maxX); out.writeDouble(box.maxY); out.writeDouble(box.maxZ);
                }
            }
        }
        exportPlayerAttributes(Path.of(args[1]));
    }

    // The player builder has different defaults and a different attribute set from the generic
    // registry. Export the actual native values, including float-to-double widening.
    private static void exportPlayerAttributes(Path path) throws Exception {
        var defaults = Player.createAttributes().build();
        var json = new StringBuilder("[\n");
        boolean first = true;
        for (var attribute : BuiltInRegistries.ATTRIBUTE) {
            var holder = BuiltInRegistries.ATTRIBUTE.wrapAsHolder(attribute);
            if (!defaults.hasAttribute(holder)) continue;
            if (!first) json.append(",\n");
            first = false;
            var ranged = (RangedAttribute)attribute;
            json.append("  {\"id\":").append(BuiltInRegistries.ATTRIBUTE.getId(attribute))
                .append(",\"name\":\"").append(BuiltInRegistries.ATTRIBUTE.getKey(attribute))
                .append("\",\"base\":").append(defaults.getBaseValue(holder))
                .append(",\"minimum\":").append(ranged.getMinValue())
                .append(",\"maximum\":").append(ranged.getMaxValue()).append('}');
        }
        Files.writeString(path, json.append("\n]\n"), StandardCharsets.UTF_8);
    }
}
