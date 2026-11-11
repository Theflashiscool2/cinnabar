// Isolated native tick/shape oracle: empty neighbours/entities, never a running server.
import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.server.level.ServerLevel;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.entity.ShulkerBoxBlockEntity;
import net.minecraft.world.level.block.state.BlockState;
import net.minecraft.world.level.redstone.Orientation;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.phys.AABB;
import sun.misc.Unsafe;
public final class ProbeShulker {
 public static class EmptyLevel extends ServerLevel {
  EmptyLevel(){super(null,null,null,null,null,null,false,0,java.util.List.of(),false);}
  @Override public void updateNeighborsAt(BlockPos p,Block block){}
  @Override public void updateNeighborsAt(BlockPos p,Block block,Orientation o){}
  @Override public void neighborShapeChanged(Direction direction,BlockPos p,BlockPos neighbour,BlockState state,int flags,int depth){}
  @Override public java.util.List<Entity> getEntities(Entity except,AABB box){return java.util.List.of();}
 }
 static JsonArray bounds(AABB b){var a=new JsonArray();for(double v:new double[]{b.minX,b.minY,b.minZ,b.maxX,b.maxY,b.maxZ})a.add(v);return a;}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var field=Unsafe.class.getDeclaredField("theUnsafe");field.setAccessible(true);var unsafe=(Unsafe)field.get(null);
  var level=(EmptyLevel)unsafe.allocateInstance(EmptyLevel.class);
  var state=Blocks.SHULKER_BOX.defaultBlockState();var box=new ShulkerBoxBlockEntity(BlockPos.ZERO,state);
  var rows=new JsonArray();
  int[] events={2,1,-1,-1,3,-1,0,-1,1,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,2,0,-1,-1,-1,-1,-1,-1,-1,-1,-1,-1,1,0,1,3,-1,0,-1,-1,-1};
  for(int event:events){
   if(event>=0)box.triggerEvent(1,event);
   ShulkerBoxBlockEntity.tick(level,BlockPos.ZERO,state,box);
   var row=new JsonObject();row.addProperty("event",event);
   var samples=new JsonArray();for(float partial:new float[]{0,0.25f,0.5f,1})samples.add(box.getProgress(partial));row.add("samples",samples);
   var shapes=new JsonArray();for(var facing:Direction.values())shapes.add(bounds(net.minecraft.world.phys.shapes.Shapes.create(box.getBoundingBox(state.setValue(ShulkerBoxBlock.FACING,facing))).bounds()));row.add("bounds",shapes);rows.add(row);
  }
  var result=new JsonObject();result.addProperty("reference","Official 26.3 ShulkerBoxBlockEntity.triggerEvent/tick/getProgress/getBoundingBox with no neighbouring blocks or entities");result.add("ticks",rows);
  Files.writeString(Path.of(args[0]),new Gson().toJson(result)+"\n");
 }
}
