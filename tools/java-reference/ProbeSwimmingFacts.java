// Original independent calls into the verified native Avatar and fluid-state registry.
import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.block.*;

public class ProbeSwimmingFacts implements net.minecraft.world.level.BlockGetter {
 final Map<net.minecraft.core.BlockPos, net.minecraft.world.level.block.state.BlockState> blocks=new HashMap<>();
 public net.minecraft.world.level.block.state.BlockState getBlockState(net.minecraft.core.BlockPos p) { return blocks.getOrDefault(p,Blocks.AIR.defaultBlockState()); }
 public net.minecraft.world.level.material.FluidState getFluidState(net.minecraft.core.BlockPos p) { return getBlockState(p).getFluidState(); }
 public net.minecraft.world.level.block.entity.BlockEntity getBlockEntity(net.minecraft.core.BlockPos p) {return null;}
 public int getHeight() {return 32;} public int getMinY() {return -16;}

 public static void main(String[] args) throws Exception {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  var version=JsonParser.parseReader(new java.io.InputStreamReader(ProbeSwimmingFacts.class.getResourceAsStream("/version.json"), java.nio.charset.StandardCharsets.UTF_8)).getAsJsonObject();
  var result=new JsonObject(); result.add("game_version",version.get("id"));result.add("protocol",version.get("protocol_version")); var field=Avatar.class.getDeclaredField("POSES"); field.setAccessible(true);
  var map=(Map<Pose,EntityDimensions>)field.get(null); var poses=new JsonObject();
  for(var pose:List.of(Pose.STANDING,Pose.CROUCHING,Pose.SWIMMING)) {
   var d=map.get(pose); var a=new JsonArray(); a.add(d.width());a.add(d.height());a.add(d.eyeHeight());poses.add(pose.name(),a);
  }
  result.add("poses",poses); int count=0;
  for(var block:Block.BLOCK_STATE_REGISTRY) {
   var f=block.getFluidState(); if(f.isEmpty()) continue; count++;
   boolean derived=!f.is(net.minecraft.tags.FluidTags.WATER)&&!f.is(net.minecraft.tags.FluidTags.LAVA)?f.isSource():
    f.getOwnHeight()==8f/9f && !f.getValue(net.minecraft.world.level.material.FlowingFluid.FALLING);
   if(derived!=f.isSource())throw new AssertionError(block.toString());
  }
  result.addProperty("source_classification_states",count);
  var cameras=new JsonArray(); var origin=new net.minecraft.core.BlockPos(8,0,8);
  for(var name:List.of("open","stone","top_slab","bottom_slab","glass","leaves","flowing_stone","falling_stone")) {
   var world=new ProbeSwimmingFacts();
   var water=Blocks.WATER.defaultBlockState().setValue(LiquidBlock.LEVEL,name.startsWith("flowing")?1:name.startsWith("falling")?8:0);
   var ceiling=switch(name) {
    case "open" -> Blocks.AIR.defaultBlockState();
    case "top_slab" -> Blocks.OAK_SLAB.defaultBlockState().setValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.SLAB_TYPE,net.minecraft.world.level.block.state.properties.SlabType.TOP);
    case "bottom_slab" -> Blocks.OAK_SLAB.defaultBlockState();
    case "glass" -> Blocks.GLASS.defaultBlockState();
    case "leaves" -> Blocks.OAK_LEAVES.defaultBlockState();
    default -> Blocks.STONE.defaultBlockState();
   };
   world.blocks.put(origin,water);world.blocks.put(origin.above(),ceiling);
   var row=new JsonObject();row.addProperty("name",name);row.addProperty("water",Block.getId(water));row.addProperty("above",Block.getId(ceiling));
   row.addProperty("height",water.getFluidState().getHeightForCamera(world,origin));cameras.add(row);
  }
  result.add("camera_cases",cameras);
  Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
 }
}
