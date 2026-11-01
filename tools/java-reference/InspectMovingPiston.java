// Behaviour probe for the pinned, locally verified Java client. No game window is started.
import java.lang.reflect.Field;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.piston.PistonMovingBlockEntity;

public final class InspectMovingPiston {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Field previous = PistonMovingBlockEntity.class.getDeclaredField("progressO");
        Field current = PistonMovingBlockEntity.class.getDeclaredField("progress");
        previous.setAccessible(true);
        current.setAccessible(true);
        if (args.length == 1 && args[0].equals("--snapshots")) {
            var registries = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
            for (String input : new String[]{"{}", "{facing:-1}", "{facing:6}",
                    "{facing:256}", "{extending:256,source:256}",
                    "{extending:1,source:1}", "{progress:0.5f,facing:5}",
                    "{facing:-1.5f,extending:-0.75f,source:0.75d}",
                    "{blockState:\"minecraft:stone\"}"}) {
                var piston = new PistonMovingBlockEntity(BlockPos.ZERO, Blocks.MOVING_PISTON.defaultBlockState());
                piston.loadCustomOnly(net.minecraft.world.level.storage.TagValueInput.create(
                    net.minecraft.util.ProblemReporter.DISCARDING, registries,
                    net.minecraft.nbt.TagParser.parseCompoundFully(input)));
                System.out.println(input + " -> " + piston.getUpdateTag(registries));
            }
            return;
        }
        for (Direction direction : Direction.values()) {
            for (boolean extending : new boolean[]{false, true}) {
                var piston = new PistonMovingBlockEntity(BlockPos.ZERO,
                    Blocks.MOVING_PISTON.defaultBlockState(), Blocks.STONE.defaultBlockState(),
                    direction, extending, false);
                for (float start : new float[]{0, .5f, 1}) {
                    previous.setFloat(piston, start);
                    current.setFloat(piston, Math.min(1, start + .5f));
                    for (float partial : new float[]{0, .25f, .5f, 1}) {
                        System.out.printf(java.util.Locale.ROOT,
                            "%d %b %.2f %.2f %.4f %.4f %.4f %.4f%n",
                            direction.get3DDataValue(), extending, start, partial,
                            piston.getProgress(partial), piston.getXOff(partial),
                            piston.getYOff(partial), piston.getZOff(partial));
                    }
                }
            }
        }
    }
}
