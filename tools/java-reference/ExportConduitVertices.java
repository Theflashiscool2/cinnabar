// Observational probe: native ModelPart vertices under the pinned renderer's transforms.
import com.mojang.blaze3d.vertex.*;
import com.mojang.math.Axis;
import net.minecraft.client.renderer.blockentity.ConduitRenderer;
import net.minecraft.client.model.geom.ModelPart;
import org.joml.*;
import java.util.*;
public class ExportConduitVertices {
 public static void main(String[] args) {
  for(int test=0;test<4;test++) {
   float time=new float[]{0,15,80,150}[test],rotation=time*-.0375f;
   var parts=new ModelPart[]{ConduitRenderer.createShellLayer().bakeRoot(),ConduitRenderer.createCageLayer().bakeRoot(),ConduitRenderer.createWindLayer().bakeRoot(),ConduitRenderer.createEyeLayer().bakeRoot()};
   for(int draw=0;draw<(test==0?1:4);draw++) {
    var pose=new PoseStack();int part;
    if(test==0){part=0;pose.translate(.5,.5,.5);pose.rotate(Axis.YP.rotationDegrees(rotation));}
    else {
     float bob=(float)java.lang.Math.sin(time*.1f)/2+.5f;
     if(draw==0){part=1;pose.translate(.5f,.3f+(bob*bob+bob)*.2f,.5f);pose.rotate(new Quaternionf().rotationAxis(rotation,new Vector3f(.5f,1,.5f).normalize()));}
     else if(draw==1){part=2;pose.translate(.5,.5,.5);int phase=((int)time/66)%3;if(phase==1)pose.rotate(Axis.XP.rotation((float)java.lang.Math.PI/2));if(phase==2)pose.rotate(Axis.ZP.rotation((float)java.lang.Math.PI/2));}
     else if(draw==2){part=2;pose.translate(.5,.5,.5);pose.scale(.875f,.875f,.875f);pose.rotate(new Quaternionf().rotationXYZ((float)java.lang.Math.PI,0,(float)java.lang.Math.PI));}
     else {part=3;pose.translate(.5f,.3f+(bob*bob+bob)*.2f,.5f);pose.scale(.5f,.5f,.5f);pose.rotate(new Quaternionf().rotationY(.7f).rotateX(-.2f));pose.rotate(new Quaternionf().rotationZ((float)java.lang.Math.PI).rotateY((float)java.lang.Math.PI));pose.scale(4f/3,4f/3,4f/3);}
    }
    var out=new ExportStatueVertices.Capture();parts[part].render(pose,out,0,0);
    System.out.printf("%d %d%n",test,draw);
    for(int i=0;i<out.vertices.size();i++){var v=out.vertices.get(i);var n=out.normals.get(i);System.out.printf(Locale.ROOT,"%.8f %.8f %.8f %.8f %.8f %.8f %.8f %.8f%n",v[0],v[1],v[2],v[3],v[4],n[0],n[1],n[2]);}
    System.out.println("END");
   }
  }
 }
}
