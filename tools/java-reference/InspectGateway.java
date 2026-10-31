// Original observational probe against the verified pinned client; no implementation copied.
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.core.Direction;
import net.minecraft.world.item.DyeColor;
public class InspectGateway {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  int states=0,hidden=0,mismatches=0;
  for(var state:Block.BLOCK_STATE_REGISTRY){
   int id=states++; int mask=0;
   for(var face:Direction.values()) if(!Block.shouldRenderFace(Blocks.END_GATEWAY.defaultBlockState(),state,face)){
    hidden++; mask |= 1 << face.ordinal();
   }
   int expected=0; for(var face:Direction.values()) if(state.getFaceOcclusionShape(face.getOpposite())==net.minecraft.world.phys.shapes.Shapes.block()) expected|=1<<face.ordinal(); if(mask!=expected)mismatches++; System.out.println("MASK "+id+" "+mask);
  }
  System.out.println("states="+states+" hiddenfaces="+hidden+" identity_mismatches="+mismatches);
  System.out.printf("magenta=%08x purple=%08x%n",DyeColor.MAGENTA.getTextureDiffuseColor(),DyeColor.PURPLE.getTextureDiffuseColor());
 }
}
