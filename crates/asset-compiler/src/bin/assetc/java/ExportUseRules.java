// Original offline dispatch-facts exporter, only for the hash-verified pinned runtime.
// Class ownership is data, never a request for the native client to load a JVM/class.
import com.google.gson.*;
import java.lang.reflect.Method;
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
import net.minecraft.world.level.block.state.*;
import net.minecraft.world.phys.BlockHitResult;
public final class ExportUseRules {
 static Class<?> owner(Class<?> type,String method,Class<?>... parameters) throws Exception {
  for (Class<?> c=type;c!=null;c=c.getSuperclass()) {
   try { c.getDeclaredMethod(method,parameters); return c; }
   catch(NoSuchMethodException ignored) {}
  }
  throw new NoSuchMethodException(type.getName()+"."+method);
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
   var row=new JsonArray();row.add(BuiltInRegistries.BLOCK.getKey(block).toString());row.add(item==BlockBehaviour.class);row.add(empty==BlockBehaviour.class);blocks.add(row);
  }
  var items=new JsonArray();
  for(var item:BuiltInRegistries.ITEM) {
   var on=owner(item.getClass(),"useOn",UseOnContext.class);
   var air=owner(item.getClass(),"use",Level.class,Player.class,InteractionHand.class);
   var living=owner(item.getClass(),"interactLivingEntity",ItemStack.class,Player.class,LivingEntity.class,InteractionHand.class);
   var row=new JsonArray();row.add(BuiltInRegistries.ITEM.getKey(item).toString());row.add(on.getSimpleName());row.add(air.getSimpleName());row.add(living.getSimpleName());items.add(row);
  }
  root.add("blocks",blocks);root.add("items",items);
  Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(root)+"\n",StandardCharsets.UTF_8);
 }
}
