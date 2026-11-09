// Original offline oracle: actual native collision restitution and slime step callbacks.
import com.google.gson.*;
import java.nio.file.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.Vec3;
import sun.misc.Unsafe;
public final class ProbeSlime {
 public static class Subject extends ArmorStand {
  double gravity;float drag;boolean shift;
  Subject(){super(null,0,0,0);}
  @Override protected double getEffectiveGravity(){return gravity;}
  @Override protected float getAirDrag(){return drag;}
  @Override protected double getEntityBounciness(){return 0;}
  @Override public boolean isSuppressingBounce(){return shift;}
  @Override public boolean isSteppingCarefully(){return shift;}
  @Override public BlockPos getBlockPosBelowThatAffectsMyMovement(){return BlockPos.ZERO;}
  @Override public void gameEvent(Holder<GameEvent> event){}
 }
 static JsonArray vector(Vec3 v){var a=new JsonArray();a.add(v.x);a.add(v.y);a.add(v.z);return a;}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var access=Unsafe.class.getDeclaredField("theUnsafe");access.setAccessible(true);var unsafe=(Unsafe)access.get(null);
  var collide=Entity.class.getDeclaredMethod("restituteMovementAfterCollisions",BlockState.class,boolean.class,boolean.class,Vec3.class);collide.setAccessible(true);
  var vertical=Entity.class.getDeclaredField("verticalCollision");vertical.setAccessible(true);
  var below=Entity.class.getDeclaredField("verticalCollisionBelow");below.setAccessible(true);
  var rows=new JsonArray();
  for(var block:new Block[]{Blocks.SLIME_BLOCK,Blocks.RED_BED})for(boolean shift:new boolean[]{false,true})for(double gravity:new double[]{0.08,0.01})
   for(double speed:new double[]{0.04,0.08,0.08000000000000002,0.5,1.3})for(double fraction:new double[]{0,0.25,0.9}) {
    var e=(Subject)unsafe.allocateInstance(Subject.class);e.gravity=gravity;e.drag=0.98f;e.shift=shift;
    var incoming=new Vec3(0.125,-speed,-0.25);var actual=new Vec3(0.125,-speed*fraction,-0.25);
    e.setDeltaMovement(incoming);vertical.setBoolean(e,true);below.setBoolean(e,true);
    collide.invoke(e,block.defaultBlockState(),false,false,actual);
    var row=new JsonObject();row.addProperty("block",net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block).toString());row.addProperty("shift",shift);row.addProperty("gravity",gravity);row.addProperty("air_drag",e.drag);
    row.add("incoming",vector(incoming));row.add("actual",vector(actual));row.addProperty("restitution",block.getBounceRestitution());row.add("result",vector(e.getDeltaMovement()));rows.add(row);
   }
  var steps=new JsonArray();for(boolean shift:new boolean[]{false,true})for(double vy:new double[]{-0.1,-0.099,-0.0784000015258789,0,0.05,0.1,0.3}) {
   var e=(Subject)unsafe.allocateInstance(Subject.class);e.shift=shift;e.setDeltaMovement(new Vec3(0.2,vy,-0.3));
   Blocks.SLIME_BLOCK.stepOn(null,BlockPos.ZERO,Blocks.SLIME_BLOCK.defaultBlockState(),e);
   var row=new JsonObject();row.addProperty("shift",shift);row.addProperty("vertical",vy);row.add("result",vector(e.getDeltaMovement()));steps.add(row);
  }
  var out=new JsonObject();out.addProperty("reference","Actual pinned Entity.restituteMovementAfterCollisions and SlimeBlock.stepOn; LivingEntity subject, no world damage or fabricated collision results");out.add("landing",rows);out.add("stepping",steps);
  Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
 }
}
