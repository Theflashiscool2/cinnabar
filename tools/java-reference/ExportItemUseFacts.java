// Original offline export of native item-use duration and cooldown-group defaults.
import com.google.gson.*;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.item.ItemCooldowns;

public final class ExportItemUseFacts {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var registries=net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries)
            .forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var version=JsonParser.parseReader(new InputStreamReader(
            ExportItemUseFacts.class.getResourceAsStream("/version.json"),StandardCharsets.UTF_8)).getAsJsonObject();
        var out=new JsonObject();out.addProperty("schema",1);
        out.add("game_version",version.get("id"));out.add("protocol",version.get("protocol_version"));
        var items=new JsonArray();var cooldowns=new ItemCooldowns();
        for(var item:BuiltInRegistries.ITEM){
            if(BuiltInRegistries.ITEM.getId(item)!=items.size())throw new IllegalStateException("Noncontiguous registry");
            var stack=item.getDefaultInstance();var row=new JsonArray();
            row.add(BuiltInRegistries.ITEM.getKey(item).toString());
            // Fail explicitly if a future item requires a non-null user context here.
            int duration=stack.getUseDuration(null);
            if(duration<0)throw new IllegalStateException("Negative native use duration");
            row.add(duration);row.add(cooldowns.getCooldownGroup(stack).toString());items.add(row);
        }
        out.add("items",items);
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
