// Original offline dispatch-facts exporter, only for the hash-verified pinned runtime.
// Class ownership is data, never a request for the native client to load a JVM/class.
import com.google.gson.*;
import java.lang.reflect.*;
import java.lang.classfile.*;
import java.lang.classfile.instruction.FieldInstruction;
import java.util.*;
import java.util.jar.JarFile;
import net.minecraft.world.entity.*;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.player.Player;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.item.*;
import net.minecraft.world.item.context.UseOnContext;
import net.minecraft.world.level.Level;
import net.minecraft.world.level.LevelReader;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.item.context.BlockPlaceContext;
import net.minecraft.world.flag.FeatureFlags;
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.phys.BlockHitResult;
import net.minecraft.world.level.border.WorldBorder;
public final class ExportUseRules {
 static Class<?> owner(Class<?> type,String method,Class<?>... parameters) throws Exception {
  for (Class<?> c=type;c!=null;c=c.getSuperclass()) {
   try { c.getDeclaredMethod(method,parameters); return c; }
   catch(NoSuchMethodException ignored) {}
  }
  throw new NoSuchMethodException(type.getName()+"."+method);
 }
 // A boolean instance field starts false. If no PUTFIELD owner in the
 // verified runtime can accept this entity class, native code can never turn
 // blocksBuilding on for it. This derives the exclusion set, not a name list.
 static void entityObstructions(JsonObject root) throws Exception {
  var owners=new TreeSet<String>();
  var jarPath=Paths.get(Entity.class.getProtectionDomain().getCodeSource().getLocation().toURI());
  try(var jar=new JarFile(jarPath.toFile())) {
   var entries=jar.entries();
   while(entries.hasMoreElements()) {
    var entry=entries.nextElement();if(!entry.getName().endsWith(".class"))continue;
    var model=ClassFile.of().parse(jar.getInputStream(entry).readAllBytes());
    for(var method:model.methods())if(method.code().isPresent())
     for(var element:method.code().get())if(element instanceof FieldInstruction f && f.opcode()==Opcode.PUTFIELD && f.name().stringValue().equals("blocksBuilding") && f.type().stringValue().equals("Z")) owners.add(f.owner().asInternalName().replace('/','.'));
   }
  }
  var classes=new ArrayList<Class<?>>();var ownerNames=new JsonArray();
  for(var name:owners){classes.add(Class.forName(name,false,Entity.class.getClassLoader()));ownerNames.add(name);}
  var clear=new TreeSet<String>();var seen=new TreeSet<String>();
  for(var field:EntityTypes.class.getDeclaredFields())if(field.getType()==EntityType.class) {
   var type=(EntityType<?>)field.get(null);
   if(!(field.getGenericType() instanceof ParameterizedType generic) || !(generic.getActualTypeArguments()[0] instanceof Class<?> entityClass))throw new IllegalStateException("Entity class unavailable: "+field.getName());
   var name=BuiltInRegistries.ENTITY_TYPE.getKey(type).toString();seen.add(name);
   if(classes.stream().noneMatch(c->c.isAssignableFrom(entityClass)))clear.add(name);
  }
  if(seen.size()!=BuiltInRegistries.ENTITY_TYPE.size())throw new IllegalStateException("Entity registry coverage");
  var names=new JsonArray();for(var name:clear)names.add(name);
  root.addProperty("entity_count",seen.size());root.add("unobstructing_entities",names);root.add("obstruction_owners",ownerNames);
 }
 public static void main(String[] args) throws Exception {
  if(args.length!=1)throw new IllegalArgumentException("expected output path");
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var v=SharedConstants.getCurrentVersion();var root=new JsonObject();
  root.addProperty("version",v.id());root.addProperty("protocol",v.protocolVersion());
  var blocks=new JsonArray();
  for(var block:BuiltInRegistries.BLOCK) {
   var item=owner(block.getClass(),"useItemOn",ItemStack.class,BlockState.class,Level.class,BlockPos.class,Player.class,InteractionHand.class,BlockHitResult.class);
   var empty=owner(block.getClass(),"useWithoutItem",BlockState.class,Level.class,BlockPos.class,Player.class,BlockHitResult.class);
   var row=new JsonArray();row.add(BuiltInRegistries.BLOCK.getKey(block).toString());row.add(item==BlockBehaviour.class);row.add(empty==BlockBehaviour.class);
   row.add(owner(block.getClass(),"getStateForPlacement",BlockPlaceContext.class)==Block.class && owner(block.getClass(),"canSurvive",BlockState.class,LevelReader.class,BlockPos.class)==BlockBehaviour.class && block.requiredFeatures().isSubsetOf(FeatureFlags.VANILLA_SET));
   row.add(owner(block.getClass(),"canBeReplaced",BlockState.class,BlockPlaceContext.class)==BlockBehaviour.class && !block.defaultBlockState().canBeReplaced());blocks.add(row);
  }
  var items=new JsonArray();
  for(var item:BuiltInRegistries.ITEM) {
   var on=owner(item.getClass(),"useOn",UseOnContext.class);
   var air=owner(item.getClass(),"use",Level.class,Player.class,InteractionHand.class);
   var living=owner(item.getClass(),"interactLivingEntity",ItemStack.class,Player.class,LivingEntity.class,InteractionHand.class);
   var row=new JsonArray();row.add(BuiltInRegistries.ITEM.getKey(item).toString());row.add(on.getSimpleName());row.add(air.getSimpleName());row.add(living.getSimpleName());
   if(item.getClass()==BlockItem.class)row.add(BuiltInRegistries.BLOCK.getKey(((BlockItem)item).getBlock()).toString());else row.add(JsonNull.INSTANCE);items.add(row);
  }
  root.add("blocks",blocks);root.add("items",items);
  var border=new WorldBorder();var defaults=new JsonObject();var center=new JsonArray();
  center.add(border.getCenterX());center.add(border.getCenterZ());defaults.add("center",center);
  defaults.addProperty("size",border.getSize());
  var max=WorldBorder.class.getDeclaredField("absoluteMaxSize");max.setAccessible(true);
  defaults.addProperty("absolute_max",max.getInt(border));
  defaults.addProperty("warning_blocks",border.getWarningBlocks());defaults.addProperty("warning_time",border.getWarningTime());root.add("border",defaults);
  entityObstructions(root);
  Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(root)+"\n",StandardCharsets.UTF_8);
 }
}
