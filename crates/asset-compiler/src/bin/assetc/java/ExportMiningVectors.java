// Original factual regression exporter. Offline only; run against the verified target archive.
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.resources.Identifier;
import net.minecraft.world.item.ItemStack;
public final class ExportMiningVectors {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  var resources=new net.minecraft.server.packs.resources.MultiPackResourceManager(net.minecraft.server.packs.PackType.SERVER_DATA,java.util.List.of(net.minecraft.server.packs.repository.ServerPacksSource.createVanillaPackSource().fullResources()));
  var pendingTags=net.minecraft.tags.TagLoader.loadTagsForExistingRegistries(resources,net.minecraft.core.RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
  for (var pending:pendingTags) pending.apply();
  var base=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
  var keys=pendingTags.stream().map(pending->pending.key()).collect(java.util.stream.Collectors.toSet());
  var lookup=net.minecraft.core.HolderLookup.Provider.create(java.util.stream.Stream.concat(base.listRegistries().filter(registry->!keys.contains(registry.key())),pendingTags.stream().map(pending->pending.lookup())));
  for (var pending:BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup)) pending.apply();
  for (String item : new String[]{"wooden_pickaxe","iron_pickaxe","diamond_pickaxe","shears","diamond_sword","stick"}) {
   ItemStack stack=new ItemStack(BuiltInRegistries.ITEM.getValue(Identifier.withDefaultNamespace(item)));
   for (String block : new String[]{"stone","obsidian","oak_log","oak_leaves","cobweb","dirt"}) {
    var state=BuiltInRegistries.BLOCK.getValue(Identifier.withDefaultNamespace(block)).defaultBlockState();
    var tool=stack.get(net.minecraft.core.component.DataComponents.TOOL);
    var tags=tool==null?java.util.List.<String>of():tool.rules().stream().filter(rule->rule.blocks().contains(state.typeHolder())).flatMap(rule->rule.blocks().unwrapKey().stream()).map(tag->tag.location().toString()).distinct().sorted().toList();
    System.out.println("VECTOR "+item+" "+block+" "+net.minecraft.world.level.block.Block.getId(state)+" "+BuiltInRegistries.BLOCK.getId(state.getBlock())+" "+Integer.toUnsignedString(Float.floatToRawIntBits(stack.getDestroySpeed(state)))+" "+stack.isCorrectToolForDrops(state)+" "+String.join(",",tags));
   }
  }
 }
}
