// Offline oracle for Entity.isColliding used by ClientLevel.syncBlockState rollback.
import com.google.gson.*;
import java.nio.file.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.*;
import sun.misc.Unsafe;
public final class ProbeBlockRollback {
 public static class Subject extends ArmorStand {
  boolean descending;
  Subject(){super(null,0,0,0);}
  @Override public boolean isDescending(){return descending;}
  @Override public ItemStack getMainHandItem(){return ItemStack.EMPTY;}
 }
 static JsonArray vector(double... values){var a=new JsonArray();for(double v:values)a.add(v);return a;}
 static Field field(String name)throws Exception{var f=Entity.class.getDeclaredField(name);f.setAccessible(true);return f;}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var access=Unsafe.class.getDeclaredField("theUnsafe");access.setAccessible(true);var unsafe=(Unsafe)access.get(null);
  var rows=new JsonArray();
  for(var block:new Block[]{Blocks.AIR,Blocks.STONE,Blocks.STONE_SLAB,Blocks.HONEY_BLOCK,Blocks.LECTERN,Blocks.SCAFFOLDING})
   for(double y:new double[]{-0.75,-0.1,0.25,0.5,1-2e-7,1-1e-7,1-5e-8,1,1+5e-8})
    for(boolean descending:new boolean[]{false,true}) for(int shift:new int[]{0,-16,16}) {
     var e=(Subject)unsafe.allocateInstance(Subject.class);e.descending=descending;
     var position=new Vec3(0.5+shift,y+shift,0.5+shift);var bb=new AABB(0.2+shift,y+shift,0.2+shift,0.8+shift,y+0.6+shift,0.8+shift);
     field("position").set(e,position);field("bb").set(e,bb);
     var state=block.defaultBlockState();var r=new JsonObject();r.addProperty("state",Block.getId(state));r.addProperty("descending",descending);r.add("min",vector(bb.minX,bb.minY,bb.minZ));r.add("max",vector(bb.maxX,bb.maxY,bb.maxZ));r.addProperty("cell",shift);r.addProperty("collides",e.isColliding(new BlockPos(shift,shift,shift),state));rows.add(r);
    }
  var out=new JsonObject();out.addProperty("reference","Actual official 26.3 Entity.isColliding with CollisionContext.of(LivingEntity), static and scaffold states, native shape-AND boundary tolerance");out.add("cases",rows);
  Files.writeString(Path.of(args[0]),new Gson().toJson(out)+"\n");
 }
}
