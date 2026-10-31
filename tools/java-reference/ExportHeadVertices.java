import com.mojang.blaze3d.vertex.*;
import net.minecraft.client.model.geom.*;
import net.minecraft.client.model.geom.builders.*;
import net.minecraft.client.model.object.skull.*;
import org.joml.*;
import java.util.*;
public class ExportHeadVertices implements VertexConsumer {
 float[] vertex; List<float[]> vertices=new ArrayList<>();
 public VertexConsumer addVertex(float x,float y,float z){vertex=new float[]{x,y,z,0,0};vertices.add(vertex);return this;}
 public VertexConsumer setUv(float u,float v){vertex[3]=u;vertex[4]=v;return this;}
 public VertexConsumer setColor(int r,int g,int b,int a){return this;} public VertexConsumer setColor(int c){return this;}
 public VertexConsumer setUv1(int a,int b){return this;} public VertexConsumer setUv2(int a,int b){return this;} public VertexConsumer setUv3(float a,float b){return this;}
 public VertexConsumer setNormal(float a,float b,float c){return this;} public VertexConsumer setLineWidth(float a){return this;}
 public static void main(String[]args){
  for(String kind:List.of("cube","human","piglin","dragon"))for(float phase:new float[]{0,2.5f,7.5f})for(int rot:new int[]{0,3,8}){
   ModelPart root=switch(kind){case "piglin"->LayerDefinition.create(PiglinHeadModel.createHeadModel(),64,64).bakeRoot();case "dragon"->DragonHeadModel.createHeadLayer().bakeRoot();case "human"->SkullModel.createHumanoidHeadLayer().bakeRoot();default->SkullModel.createMobHeadLayer().bakeRoot();};
   SkullModelBase model=switch(kind){case "piglin"->new PiglinHeadModel(root);case "dragon"->new DragonHeadModel(root);default->new SkullModel(root);};
   var state=new SkullModelBase.State();state.animationPos=phase;model.setupAnim(state);
   var stack=new PoseStack();stack.translate(.5,0,.5);stack.mulPose(new Matrix4f().rotationY((float)java.lang.Math.toRadians(-rot*22.5)));stack.scale(-1,-1,1);
   var out=new ExportHeadVertices();root.render(stack,out,0,0);
   System.out.printf("%s %.1f %d%n",kind,phase,rot);
   for(var v:out.vertices)System.out.printf(Locale.ROOT,"%.8f %.8f %.8f %.8f %.8f%n",v[0],v[1],v[2],v[3],v[4]);
   System.out.println("END");
  }
 }
}
