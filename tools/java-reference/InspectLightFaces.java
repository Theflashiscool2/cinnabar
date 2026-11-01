// Original functional probe against the verified pinned server. No renderer or world runs.
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.Direction;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.lighting.LightEngine;
public final class InspectLightFaces {
    public static void main(String[] args) throws Exception {
        if(args.length!=1) throw new IllegalArgumentException("expected output path");
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var states=new ArrayList<BlockState>();
        for(var block:new Block[]{Blocks.AIR,Blocks.STONE,Blocks.GLASS,Blocks.OAK_SLAB,
                Blocks.OAK_STAIRS,Blocks.OAK_TRAPDOOR,Blocks.PISTON,Blocks.STICKY_PISTON,
                Blocks.PISTON_HEAD,Blocks.MOVING_PISTON,Blocks.WATER,Blocks.LAVA})
            states.addAll(block.getStateDefinition().getPossibleStates());
        try(var out=Files.newBufferedWriter(Path.of(args[0]))){
            out.write("COUNT "+states.size()+"\n");
            for(var a:states) for(var b:states) for(var direction:Direction.values())
                out.write(Block.getId(a)+" "+Block.getId(b)+" "+direction.getSerializedName()+" "+
                    (LightEngine.getLightDampeningInto(a,b,direction,0)==16)+"\n");
        }
    }
}
