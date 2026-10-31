import com.mojang.blaze3d.vertex.*;
import net.minecraft.client.model.animal.golem.CopperGolemModel;
import net.minecraft.client.model.object.statue.CopperGolemStatueModel;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.util.Unit;
import org.joml.Matrix4f;
import java.util.*;
public class ExportStatueVertices {
 static class Capture extends ExportHeadVertices { List<float[]> normals=new ArrayList<>(); public VertexConsumer setNormal(float x,float y,float z){normals.add(new float[]{x,y,z});return this;} }
 public static void main(String[]args){
  for(String kind:List.of("standing","running","sitting","star"))for(int turn:new int[]{0,1,2,3}){
   ModelPart root=switch(kind){case "running"->CopperGolemModel.createRunningPoseBodyLayer().bakeRoot();case "sitting"->CopperGolemModel.createSittingPoseBodyLayer().bakeRoot();case "star"->CopperGolemModel.createStarPoseBodyLayer().bakeRoot();default->CopperGolemModel.createBodyLayer().bakeRoot();};
   new CopperGolemStatueModel(root).setupAnim(Unit.INSTANCE);
   var stack=new PoseStack();stack.translate(.5,0,.5);stack.mulPose(new Matrix4f().rotationY((float)java.lang.Math.toRadians(-((turn+2)%4)*90)));
   var out=new Capture();root.render(stack,out,0,0);
   System.out.printf("%s %d%n",kind,turn);
   for(int i=0;i<out.vertices.size();i++){var v=out.vertices.get(i);var n=out.normals.get(i);System.out.printf(Locale.ROOT,"%.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f%n",v[0],v[1],v[2],v[3],v[4],n[0],n[1],n[2]);}
   System.out.println("END");
  }
 }
}
