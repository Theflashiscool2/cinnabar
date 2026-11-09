// Original offline factual probe. Uses only the hash-verified pinned runtime.
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.ItemStack;
public class ProbeEmptyTags {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
  for(var pending:BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup))pending.apply();
  for(String item:new String[]{"air","wooden_sword","iron_pickaxe","shears"}) {
   var stack=new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(item)));
   for(String block:new String[]{"stone","white_wool","cobweb"}) {
    var state=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(block)).defaultBlockState();
    System.out.println("EMPTY_TAG "+item+" "+block+" "+net.minecraft.world.level.block.Block.getId(state)+" "+BuiltInRegistries.BLOCK.getId(state.getBlock())+" "+Integer.toUnsignedString(Float.floatToRawIntBits(stack.getDestroySpeed(state)))+" "+stack.isCorrectToolForDrops(state));
   }
  }
 }
}
