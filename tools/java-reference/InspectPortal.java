// Observations from the supplied pinned client; no game implementation is copied.
import net.minecraft.client.renderer.RenderPipelines;
import net.minecraft.client.renderer.blockentity.TheEndPortalRenderer;
import net.minecraft.world.level.block.entity.TheEndPortalBlockEntity;
import net.minecraft.world.level.block.Blocks;
import net.minecraft.core.BlockPos;
import net.minecraft.core.Direction;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import org.joml.Vector3f;
public class InspectPortal {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  System.out.println("portal " + RenderPipelines.END_PORTAL.getShaderDefines());
  System.out.println("gateway " + RenderPipelines.END_GATEWAY.getShaderDefines());
  var block=new TheEndPortalBlockEntity(BlockPos.ZERO,Blocks.END_PORTAL.defaultBlockState());
  for(var face:Direction.values())System.out.println(face+" "+block.shouldRenderFace(face));
  for(var p:new Vector3f[]{new Vector3f(0,0,0),new Vector3f(1,1,1)})
   System.out.println("bound "+TheEndPortalRenderer.TRANSFORMATION.getMatrix().transformPosition(p));
 }
}
