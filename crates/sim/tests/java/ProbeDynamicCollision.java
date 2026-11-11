// Targeted native dynamic collision facts and independent coordinate/context vectors.
import com.google.gson.*;
import java.nio.file.*;
import java.lang.reflect.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.tags.TagKey;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.decoration.ArmorStand;
import net.minecraft.world.item.*;
import net.minecraft.world.level.*;
import net.minecraft.world.level.block.*;
import net.minecraft.world.level.block.piston.MovingPistonBlock;
import net.minecraft.world.phys.*;
import net.minecraft.world.phys.shapes.*;
import sun.misc.Unsafe;
public final class ProbeDynamicCollision {
 public static class Subject extends ArmorStand {
  boolean descending, boots, walkable;
  Subject(){super(null,0,0,0);}
  @Override public boolean isDescending(){return descending;}
  @Override public ItemStack getMainHandItem(){return ItemStack.EMPTY;}
  @Override public ItemStack getItemBySlot(EquipmentSlot slot){return boots?new ItemStack(Items.LEATHER_BOOTS):ItemStack.EMPTY;}
  @Override public boolean is(TagKey<EntityType<?>> tag){return walkable;}
 }
 static JsonArray vector(double... values){var a=new JsonArray();for(double v:values)a.add(v);return a;}
 static JsonArray position(BlockPos p){var a=new JsonArray();a.add(p.getX());a.add(p.getY());a.add(p.getZ());return a;}
 static JsonArray boxes(VoxelShape shape){var a=new JsonArray();for(var b:shape.toAabbs())a.add(vector(b.minX,b.minY,b.minZ,b.maxX,b.maxY,b.maxZ));return a;}
 static Field field(String name)throws Exception{var f=Entity.class.getDeclaredField(name);f.setAccessible(true);return f;}
 static Method maxOffset(Block block)throws Exception{for(Class<?> c=block.getClass();c!=null;c=c.getSuperclass())try{var m=c.getDeclaredMethod("getMaxHorizontalOffset");m.setAccessible(true);return m;}catch(NoSuchMethodException e){}throw new NoSuchMethodException();}
 public static void main(String[] args)throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
  for(var pending:BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup))pending.apply();
  var pin=JsonParser.parseString(Files.readString(Path.of(args[0]))).getAsJsonObject();
  var access=Unsafe.class.getDeclaredField("theUnsafe");access.setAccessible(true);var unsafe=(Unsafe)access.get(null);
  var states=new JsonObject();var cases=new JsonArray();
  for(var block:BuiltInRegistries.BLOCK) {
   String kind=block instanceof SpeleothemBlock||block instanceof BambooStalkBlock?"offset":block instanceof ShulkerBoxBlock?"shulker":block instanceof MovingPistonBlock?"moving":block instanceof PowderSnowBlock?"powder_snow":null;
   if(kind==null)continue;
   for(var state:block.getStateDefinition().getPossibleStates()) {
    int id=Block.getId(state);var fact=new JsonObject();fact.addProperty("kind",kind);
    if(kind.equals("shulker")){fact.addProperty("block_id",BuiltInRegistries.BLOCK.getId(block));fact.addProperty("facing",state.getValue(ShulkerBoxBlock.FACING).get3DDataValue());}
    var offset=state.getOffset(BlockPos.ZERO);var shape=state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,CollisionContext.empty());
    fact.add("base",boxes(shape.move(-offset.x,-offset.y,-offset.z)));
    if(kind.equals("offset"))fact.addProperty("max_offset",(float)maxOffset(block).invoke(block));
    states.add(Integer.toString(id),fact);
    if(kind.equals("powder_snow"))continue;
    for(var pos:new BlockPos[]{BlockPos.ZERO,new BlockPos(1,17,2),new BlockPos(-16,-3,27),new BlockPos(2147483647,0,-2147483648)}) {
     var row=new JsonObject();row.addProperty("state",id);row.add("position",position(pos));row.add("boxes",boxes(state.getCollisionShape(EmptyBlockGetter.INSTANCE,pos,CollisionContext.empty())));cases.add(row);
    }
   }
  }
  var powderRows=new JsonArray();
  for(double fall:new double[]{0,2.5,2.500000001,9})for(double y:new double[]{0,0.95,1,1.000001})for(boolean down:new boolean[]{false,true})for(boolean boots:new boolean[]{false,true})for(boolean walkable:new boolean[]{false,true}){
   var e=(Subject)unsafe.allocateInstance(Subject.class);e.descending=down;e.boots=boots;e.walkable=walkable;e.fallDistance=fall;field("position").set(e,new Vec3(0.5,y,0.5));field("bb").set(e,new AABB(0.2,y,0.2,0.8,y+1.8,0.8));
   var r=new JsonObject();r.addProperty("fall",fall);r.addProperty("bottom",y);r.addProperty("descending",down);r.addProperty("boots",boots);r.addProperty("walkable",walkable);r.add("boxes",boxes(Blocks.POWDER_SNOW.defaultBlockState().getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,CollisionContext.of(e))));powderRows.add(r);
  }
  var falling=PowderSnowBlock.class.getDeclaredField("FALLING_COLLISION_SHAPE");falling.setAccessible(true);
  var report=new JsonObject();report.addProperty("schema",1);report.add("game_version",pin.get("game_version"));report.add("protocol",pin.get("protocol"));report.add("blocks_sha256",pin.getAsJsonObject("block_registry_report").get("sha256"));report.add("states",pin.getAsJsonObject("block_registry_report").get("states"));report.add("dynamic",states);report.add("powder_falling",boxes((VoxelShape)falling.get(null)));report.add("powder_standing",boxes(Shapes.block()));
  Files.writeString(Path.of(args[1]),new Gson().toJson(report)+"\n");
  var out=new JsonObject();out.addProperty("reference","Official 26.3 dynamic collision methods, position offsets, missing block-entity defaults and LivingEntity powder-snow contexts");out.add("coordinate",cases);out.add("powder",powderRows);Files.writeString(Path.of(args[2]),new Gson().toJson(out)+"\n");
 }
}
