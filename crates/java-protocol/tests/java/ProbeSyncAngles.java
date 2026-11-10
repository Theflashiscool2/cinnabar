// Original native 26.3 oracle: sample the real stepped timeline and Entity setter policy.
import java.lang.reflect.*;
public class ProbeSyncAngles {
 public static void main(String[] args) throws Exception {
  Class<?> v=Class.forName("net.minecraft.world.phys.Vec3");
  Object position=v.getConstructor(double.class,double.class,double.class).newInstance(1.,64.,0.);
  Class<?> c=Class.forName("net.minecraft.world.entity.SteppedInterpolationHandler$InterpolationData");
  Constructor<?> ctor=c.getDeclaredConstructor();ctor.setAccessible(true);Object data=ctor.newInstance();
  Method start=c.getDeclaredMethod("setStartingPoint",v,float.class,float.class);start.setAccessible(true);
  Method add=c.getDeclaredMethod("addStep",v,float.class,float.class,int.class);add.setAccessible(true);
  Method sample=c.getDeclaredMethod("getNewPositionAndRotation");sample.setAccessible(true);
  Method advance=c.getDeclaredMethod("advance",float.class,int.class);advance.setAccessible(true);
  Class<?> pose=Class.forName("net.minecraft.core.PositionAndRotation");
  Method yaw=pose.getMethod("yRot"),pitch=pose.getMethod("xRot");
  Method set=c.getMethod("set",v,float.class,float.class);set.setAccessible(true);
  Field pending=c.getDeclaredField("remainingSteps");pending.setAccessible(true);
  start.invoke(data,position,0f,0f);add.invoke(data,position,90f,0f,3);set.invoke(data,position,90f,0f);
  float lastYaw=0f,lastPitch=0f;
  for(int tick=0;tick<12;tick++) {
   if(tick==1){add.invoke(data,position,Float.NaN,12f,3);set.invoke(data,position,Float.NaN,12f);}
   if(tick==7){start.invoke(data,position,lastYaw,lastPitch);add.invoke(data,position,45f,Float.POSITIVE_INFINITY,3);set.invoke(data,position,45f,Float.POSITIVE_INFINITY);}
   if(!((java.util.List<?>)pending.get(data)).isEmpty()) {
    Object p=sample.invoke(data);float y=(float)yaw.invoke(p),x=(float)pitch.invoke(p);
    if(Float.isFinite(y))lastYaw=y%360f;if(Float.isFinite(x))lastPitch=x%360f;
    advance.invoke(data,1f,3);
   }
   System.out.println(tick+","+lastYaw+","+lastPitch);
  }
 }
}
