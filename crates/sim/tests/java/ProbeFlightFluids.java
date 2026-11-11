// Actual native Player.travel with real fluid states and native liquid branch selection.
// Unimpeded move sink; excludes collision, base-tick currents and LocalPlayer input ticking.
import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.entity.player.*;
import net.minecraft.world.level.material.*;
import net.minecraft.world.phys.Vec3;
import sun.misc.Unsafe;
public final class ProbeFlightFluids {
 public static class Level extends ProbeFlight.EmptyLevel {
  FluidState fluid;
  Level(){super();}
  @Override public FluidState getFluidState(BlockPos p){return fluid;}
 }
 public static class Subject extends ProbeFlight.Subject {
  boolean water,selectedFluid; int selections;
  Subject(){super();}
  @Override public boolean isInWater(){return water;}
  @Override public boolean isInLava(){return !water;}
  @Override protected boolean shouldTravelInFluid(FluidState state){
   selections++;selectedFluid=super.shouldTravelInFluid(state);return selectedFluid;
  }
 }
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var field=Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);var unsafe=(Unsafe)field.get(null);
  var rows=new JsonArray();
  for(boolean water:new boolean[]{true,false})for(float speed:new float[]{0.05f,-0.05f})for(boolean sprint:new boolean[]{false,true})for(boolean jump:new boolean[]{false,true})for(boolean shift:new boolean[]{false,true})for(double initial:new double[]{-0.2,0,0.0029,0.003}){
   var level=(Level)unsafe.allocateInstance(Level.class);level.fluid=(water?Fluids.WATER:Fluids.LAVA).defaultFluidState();
   var e=(Subject)unsafe.allocateInstance(Subject.class);e.water=water;e.sprint=sprint;
   var a=new Abilities();a.mayfly=true;a.flying=true;a.setFlyingSpeed(speed);ProbeFlight.set(Player.class,e,"abilities",a);ProbeFlight.set(Entity.class,e,"level",level);ProbeFlight.set(Entity.class,e,"position",new Vec3(8.5,8,8.5));
   int direction=(jump?1:0)-(shift?1:0);double y=initial;if(direction!=0)y+=(float)direction*speed*3f;if(Math.abs(y)<0.003)y=0;
   e.setDeltaMovement(new Vec3(0.125,y,-0.25));e.travel(new Vec3(0,0,(double)0.98f));
   if(!e.isInLiquid()||e.isAffectedByFluids()||e.selectedFluid||e.selections!=1)throw new AssertionError("native flight fluid branch");
   var row=new JsonObject();row.addProperty("fluid",water?"water":"lava");row.addProperty("speed",speed);row.addProperty("sprint",sprint);row.addProperty("jump",jump);row.addProperty("shift",shift);row.addProperty("initial_y",initial);row.addProperty("selected_fluid",e.selectedFluid);row.add("move",ProbeFlight.vector(e.moved));row.add("velocity",ProbeFlight.vector(e.getDeltaMovement()));rows.add(row);
  }
  var out=new JsonObject();out.addProperty("reference","Official 26.3 Player.travel/isAffectedByFluids and LivingEntity.shouldTravelInFluid executed with water/lava contacts and real fluid states; LocalPlayer lift arithmetic transcribed; unimpeded move sink");out.add("cases",rows);Files.writeString(Path.of(args[0]),new Gson().toJson(out)+"\n");
 }
}
