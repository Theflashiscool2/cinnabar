import com.mojang.blaze3d.vertex.*;
import net.minecraft.client.renderer.blockentity.DecoratedPotRenderer;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.core.Direction;
import java.util.*;
public class ExportPotVertices {
 public static void main(String[] args) {
  for(var facing:List.of(Direction.SOUTH,Direction.WEST,Direction.NORTH,Direction.EAST)) {
   var base=DecoratedPotRenderer.createBaseLayer().bakeRoot();
   var sides=DecoratedPotRenderer.createSidesLayer().bakeRoot();
   for(String part:List.of("neck","top","bottom","front","back","left","right")) {
    var pose=new PoseStack();pose.mulPose(DecoratedPotRenderer.modelTransformation(facing));
    var out=new ExportStatueVertices.Capture();
    (List.of("neck","top","bottom").contains(part)?base:sides).getChild(part).render(pose,out,0,0);
    System.out.printf("%s %s%n",facing.getSerializedName(),part);
    for(int i=0;i<out.vertices.size();i++){var v=out.vertices.get(i);var n=out.normals.get(i);System.out.printf(Locale.ROOT,"%.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f%n",v[0],v[1],v[2],v[3],v[4],n[0],n[1],n[2]);}
    System.out.println("END");
   }
  }
 }
}
