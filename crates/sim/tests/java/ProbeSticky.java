// Offline behavior oracle against the hash-pinned official 26.3 server archive.
import com.google.gson.*;
import java.nio.file.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.effect.MobEffect;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.gameevent.GameEvent;
import net.minecraft.world.phys.Vec3;
import sun.misc.Unsafe;
public final class ProbeSticky {
 public static class Subject extends ArmorStand {
  double bounce; boolean shift, weaving;
  Subject(){super(null,0,0,0);}
  @Override protected double getEffectiveGravity(){return 0.08;}
  @Override protected float getAirDrag(){return 0.98f;}
  @Override protected double getEntityBounciness(){return bounce;}
  @Override public boolean isSuppressingBounce(){return shift;}
  @Override public boolean hasEffect(Holder<MobEffect> effect){return weaving;}
  @Override public BlockPos getBlockPosBelowThatAffectsMyMovement(){return BlockPos.ZERO;}
  @Override public void gameEvent(Holder<GameEvent> event){}
 }
 static JsonArray vector(Vec3 v){var a=new JsonArray();a.add(v.x);a.add(v.y);a.add(v.z);return a;}
 static Field field(String name)throws Exception{var f=Entity.class.getDeclaredField(name);f.setAccessible(true);return f;}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var access=Unsafe.class.getDeclaredField("theUnsafe");access.setAccessible(true);var unsafe=(Unsafe)access.get(null);
  var sliding=HoneyBlock.class.getDeclaredMethod("isSlidingDown",BlockPos.class,Entity.class);sliding.setAccessible(true);
  var slide=HoneyBlock.class.getDeclaredMethod("doSlideMovement",Entity.class);slide.setAccessible(true);
  var collide=Entity.class.getDeclaredMethod("restituteMovementAfterCollisions",BlockState.class,boolean.class,boolean.class,Vec3.class);collide.setAccessible(true);
  var web=WebBlock.class.getDeclaredMethod("entityInside",BlockState.class,net.minecraft.world.level.Level.class,BlockPos.class,Entity.class,InsideBlockEffectApplier.class,boolean.class);web.setAccessible(true);
  var honeyRows=new JsonArray();
  for(double x:new double[]{0.5,0.7375,0.7375001,1.})for(double y:new double[]{0.5,0.9374999,0.9375})for(double vy:new double[]{-0.08,-0.13,-0.2,-1.})for(boolean grounded:new boolean[]{false,true}) {
   var e=(Subject)unsafe.allocateInstance(Subject.class);
   field("position").set(e,new Vec3(x,y,0.5));field("dimensions").set(e,EntityDimensions.scalable(0.6f,1.8f));field("onGround").setBoolean(e,grounded);
   e.setDeltaMovement(new Vec3(0.2,vy,-0.3));e.fallDistance=9;
   boolean applies=(boolean)sliding.invoke(Blocks.HONEY_BLOCK,BlockPos.ZERO,e);if(applies)slide.invoke(Blocks.HONEY_BLOCK,e);
   var r=new JsonObject();r.add("position",vector(new Vec3(x,y,0.5)));r.addProperty("grounded",grounded);r.addProperty("vertical",vy);r.add("result",vector(e.getDeltaMovement()));r.addProperty("fall",e.fallDistance);honeyRows.add(r);
  }
  var bounceRows=new JsonArray();
  for(boolean shift:new boolean[]{false,true})for(boolean below:new boolean[]{false,true})for(double bounce:new double[]{0,0.3,1.2})for(double vy:new double[]{-0.04,-0.8,0.8}) {
   var e=(Subject)unsafe.allocateInstance(Subject.class);e.shift=shift;e.bounce=bounce;e.setDeltaMovement(new Vec3(0.2,vy,-0.3));field("verticalCollision").setBoolean(e,true);field("verticalCollisionBelow").setBoolean(e,below);
   var actual=new Vec3(0.1,vy*0.25,-0.15);collide.invoke(e,Blocks.STONE.defaultBlockState(),true,true,actual);
   var r=new JsonObject();r.addProperty("shift",shift);r.addProperty("below",below);r.addProperty("bounce",bounce);r.addProperty("vertical",vy);r.add("result",vector(e.getDeltaMovement()));bounceRows.add(r);
  }
  var webRows=new JsonArray();for(boolean weaving:new boolean[]{false,true}){
   var e=(Subject)unsafe.allocateInstance(Subject.class);e.weaving=weaving;e.fallDistance=9;web.invoke(Blocks.COBWEB,Blocks.COBWEB.defaultBlockState(),null,BlockPos.ZERO,e,null,false);
   var r=new JsonObject();r.addProperty("weaving",weaving);r.add("multiplier",vector((Vec3)field("stuckSpeedMultiplier").get(e)));r.addProperty("fall",e.fallDistance);webRows.add(r);
  }
  var out=new JsonObject();out.addProperty("reference","Official 26.3 HoneyBlock, WebBlock and Entity restitution methods invoked on isolated LivingEntity subjects");out.add("honey",honeyRows);out.add("bounce",bounceRows);out.add("web",webRows);
  Files.writeString(Path.of(args[0]),new Gson().toJson(out)+"\n");
 }
}
