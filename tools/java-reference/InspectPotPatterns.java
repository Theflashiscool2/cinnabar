import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.core.component.DataComponents;
public class InspectPotPatterns {
 public static void main(String[]args){SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var lookup=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
 for(var pending:BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup)) {
  if(pending.key().equals(net.minecraft.core.registries.Registries.ITEM)) pending.forEach((item,components)->{var p=components.get(DataComponents.PROVIDES_POTTERY_PATTERN);if(p!=null)System.out.println(item.key().identifier()+" "+p.unwrapKey().map(k->k.identifier().toString()).orElse("DIRECT"));});
 }

 for(var style:net.minecraft.world.level.block.entity.DecoratedPotBlockEntity.WobbleStyle.values())System.out.println("WOBBLE "+style+" "+style.duration);
 }
}
