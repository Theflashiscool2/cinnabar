// Original behaviour probe; generated native shapes stay outside source control.
import com.google.gson.Gson;
import java.lang.reflect.Field;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import net.minecraft.SharedConstants;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.world.level.block.piston.PistonBaseBlock;
import net.minecraft.world.level.block.piston.PistonHeadBlock;
import net.minecraft.world.level.block.piston.PistonMovingBlockEntity;
import net.minecraft.world.level.block.state.properties.PistonType;

public final class InspectPistonCollision {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Field current = PistonMovingBlockEntity.class.getDeclaredField("progress");
        current.setAccessible(true);
        var gson = new Gson();
        for (Direction d : Direction.values()) for (boolean extending : new boolean[]{false, true}) {
            for (int mode = 0; mode < 4; mode++) {
                boolean source = mode >= 2;
                var moved = mode == 0 ? Blocks.STONE.defaultBlockState() : Blocks.OAK_SLAB.defaultBlockState();
                if (source) moved = extending
                    ? Blocks.PISTON_HEAD.defaultBlockState().setValue(PistonHeadBlock.FACING, d)
                        .setValue(PistonHeadBlock.TYPE, mode == 3 ? PistonType.STICKY : PistonType.DEFAULT)
                    : (mode == 3 ? Blocks.STICKY_PISTON : Blocks.PISTON).defaultBlockState()
                        .setValue(PistonBaseBlock.FACING, d);
                var piston = new PistonMovingBlockEntity(BlockPos.ZERO, Blocks.MOVING_PISTON.defaultBlockState(), moved, d, extending, source);
                for (float progress : new float[]{0, .25f, .5f, .75f, .875f, 1}) {
                    current.setFloat(piston, progress);
                    var boxes = new ArrayList<double[]>();
                    piston.getCollisionShape(EmptyBlockGetter.INSTANCE, BlockPos.ZERO).forAllBoxes((x0,y0,z0,x1,y1,z1) -> boxes.add(new double[]{x0,y0,z0,x1,y1,z1}));
                    var row = new LinkedHashMap<String,Object>();
                    row.put("facing", d.get3DDataValue()); row.put("extending", extending);
                    row.put("source", source); row.put("state", Block.getId(moved));
                    row.put("progress", progress); row.put("boxes", boxes);
                    System.out.println(gson.toJson(row));
                }
            }
        }
    }
}
