// Original observation harness: calls the verified native renderer with a vertex collector.
import com.mojang.blaze3d.vertex.*;
import net.minecraft.client.renderer.SubmitNodeCollector;
import net.minecraft.client.renderer.blockentity.BeaconRenderer;
import net.minecraft.resources.Identifier;
import java.lang.reflect.Proxy;
import java.util.Locale;
public class ExportGatewayVertices {
 public static void main(String[] args) {
  for(int test=0;test<3;test++) {
   final int caseId=test;
   var collector=(SubmitNodeCollector)Proxy.newProxyInstance(SubmitNodeCollector.class.getClassLoader(),new Class[]{SubmitNodeCollector.class},(proxy,method,a)->{
    if(method.getName().equals("submitCustomGeometry")){
     var output=new ExportHeadVertices();
     ((SubmitNodeCollector.CustomGeometryRenderer)a[2]).render(((PoseStack)a[0]).last(),output);
     System.out.println("CASE "+caseId);
     for(var v:output.vertices)System.out.printf(Locale.ROOT,"%.8f %.8f %.8f %.8f %.8f%n",v[0],v[1],v[2],v[3],v[4]);
     System.out.println("END");
    }
    return null;
   });
   BeaconRenderer.submitBeaconBeam(new PoseStack(),collector,Identifier.withDefaultNamespace("textures/entity/end_portal/end_gateway_beam.png"),new float[]{1,.5f,.25f}[test],new float[]{20,0,37.5f}[test],-50,100,0xff8932b8,.15f,.175f);
  }
 }
}
