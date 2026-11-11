// Native Player air-travel oracle on an unimpeded move sink; not a live server/world.
import com.google.gson.*;
import java.nio.file.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.ai.attributes.*;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.effect.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.material.*;
import net.minecraft.world.phys.*;
import sun.misc.Unsafe;
public final class ProbeFlight {
 public static class EmptyLevel extends ProbeShulker.EmptyLevel {
  EmptyLevel(){super();}
  @Override public net.minecraft.world.level.block.state.BlockState getBlockState(BlockPos p){return Blocks.AIR.defaultBlockState();}
  @Override public FluidState getFluidState(BlockPos p){return Fluids.EMPTY.defaultFluidState();}
 }
 public static class Subject extends Player {
  boolean sprint; Vec3 moved;
  Subject(){super(null,null);}
  @Override public GameType gameMode(){return GameType.CREATIVE;}
  @Override public boolean isSwimming(){return false;}
  @Override public boolean isSprinting(){return sprint;}
  @Override public boolean isInWater(){return false;}
  @Override public boolean isInLava(){return false;}
  @Override public boolean isFallFlying(){return false;}
  @Override public boolean isNoGravity(){return false;}
  @Override public boolean onClimbable(){return false;}
  @Override public BlockPos getBlockPosBelowThatAffectsMyMovement(){return BlockPos.ZERO;}
  @Override public boolean hasEffect(Holder<MobEffect> effect){return false;}
  @Override public MobEffectInstance getEffect(Holder<MobEffect> effect){return null;}
  @Override public double getAttributeValue(Holder<Attribute> attribute){
   if(attribute==Attributes.GRAVITY)return 0.08;
   if(attribute==Attributes.MOVEMENT_SPEED)return 0.1;
   return 1;
  }
  @Override public void move(MoverType kind,Vec3 movement){moved=movement;}
 }
 static JsonArray vector(Vec3 v){var a=new JsonArray();a.add(v.x);a.add(v.y);a.add(v.z);return a;}
 static void set(Class<?> owner,Object subject,String name,Object value)throws Exception{var f=owner.getDeclaredField(name);f.setAccessible(true);f.set(subject,value);}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var f=Unsafe.class.getDeclaredField("theUnsafe");f.setAccessible(true);var unsafe=(Unsafe)f.get(null);
  var level=(EmptyLevel)unsafe.allocateInstance(EmptyLevel.class);var rows=new JsonArray();
  var flyingSpeed=Player.class.getDeclaredMethod("getFlyingSpeed");flyingSpeed.setAccessible(true);
  for(float speed:new float[]{0,0.05f,0.01337f,-0.05f})for(boolean sprint:new boolean[]{false,true})for(boolean jump:new boolean[]{false,true})for(boolean shift:new boolean[]{false,true})for(double initial:new double[]{-0.2,0,0.0029,0.003}){
   var e=(Subject)unsafe.allocateInstance(Subject.class);e.sprint=sprint;
   var a=new Abilities();a.mayfly=true;a.flying=true;a.setFlyingSpeed(speed);set(Player.class,e,"abilities",a);set(Entity.class,e,"level",level);
   set(Entity.class,e,"position",new Vec3(8.5,8,8.5));
   // LocalPlayer.aiStep's identified float arithmetic precedes LivingEntity's velocity cutoff.
   int direction=(jump?1:0)-(shift?1:0);double y=initial;if(direction!=0)y+=(float)direction*speed*3f;if(Math.abs(y)<0.003)y=0;
   e.setDeltaMovement(new Vec3(0.125,y,-0.25));
   e.travel(new Vec3(0,0,(double)0.98f));
   var row=new JsonObject();row.addProperty("speed",speed);row.addProperty("sprint",sprint);row.addProperty("jump",jump);row.addProperty("shift",shift);row.addProperty("initial_y",initial);row.addProperty("air_speed",(float)flyingSpeed.invoke(e));row.add("move",vector(e.moved));row.add("velocity",vector(e.getDeltaMovement()));rows.add(row);
  }
  var packets=new JsonArray();for(boolean flying:new boolean[]{false,true}){var a=new Abilities();a.flying=flying;var b=new net.minecraft.network.FriendlyByteBuf(io.netty.buffer.Unpooled.buffer());net.minecraft.network.protocol.game.ServerboundPlayerAbilitiesPacket.STREAM_CODEC.encode(b,new net.minecraft.network.protocol.game.ServerboundPlayerAbilitiesPacket(a));var row=new JsonObject();row.addProperty("flying",flying);row.addProperty("byte",b.readUnsignedByte());packets.add(row);b.release();}
  var out=new JsonObject();out.addProperty("reference","Official 26.3 Player.travel/getFlyingSpeed and ServerboundPlayerAbilitiesPacket codec invoked; LocalPlayer.aiStep vertical float arithmetic transcribed from identified bytecode; unimpeded move sink, no terrain-collision claim");out.add("air",rows);out.add("packets",packets);Files.writeString(Path.of(args[0]),new Gson().toJson(out)+"\n");
 }
}
