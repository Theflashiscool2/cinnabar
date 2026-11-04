// Original offline export of native inventory rules/default name components; no network.
import com.google.gson.*;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.SimpleContainer;
import net.minecraft.world.inventory.ContainerInput;

public final class ExportInventoryFacts {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var registries = net.minecraft.data.registries.VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(registries)
            .forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var version = JsonParser.parseReader(new InputStreamReader(
            ExportInventoryFacts.class.getResourceAsStream("/version.json"),
            StandardCharsets.UTF_8)).getAsJsonObject();
        var out = new JsonObject();
        out.addProperty("schema", 3);
        out.add("game_version", version.get("id"));
        out.add("protocol", version.get("protocol_version"));
        out.addProperty("empty_item", BuiltInRegistries.ITEM.getId(net.minecraft.world.item.Items.AIR));
        out.addProperty("container_slot_limit", new SimpleContainer(27).getMaxStackSize());
        var maxSlots = net.minecraft.network.protocol.game.ServerboundContainerClickPacket.class
            .getDeclaredField("MAX_SLOT_COUNT");
        maxSlots.setAccessible(true);
        out.addProperty("max_changed_slots", maxSlots.getInt(null));
        var inputs = new JsonObject();
        for (var input : ContainerInput.values()) inputs.addProperty(input.name(), input.id());
        out.add("inputs", inputs);
        var items = new JsonArray();
        for (var item : BuiltInRegistries.ITEM) {
            if (BuiltInRegistries.ITEM.getId(item) != items.size()) {
                throw new IllegalStateException("non-contiguous native item registry");
            }
            var entry = new JsonArray();
            entry.add(BuiltInRegistries.ITEM.getKey(item).toString());
            entry.add(item.getDefaultMaxStackSize());
            var name = item.getDefaultInstance().get(net.minecraft.core.component.DataComponents.ITEM_NAME);
            entry.add(name == null ? JsonNull.INSTANCE : net.minecraft.network.chat.ComponentSerialization.CODEC
                .encodeStart(registries.createSerializationContext(com.mojang.serialization.JsonOps.INSTANCE), name)
                .getOrThrow());
            var stack = item.getDefaultInstance();
            var attack = new JsonObject();
            attack.addProperty("range", stack.has(net.minecraft.core.component.DataComponents.ATTACK_RANGE));
            attack.addProperty("piercing", stack.has(net.minecraft.core.component.DataComponents.PIERCING_WEAPON));
            attack.addProperty("minimum_charge", stack.getOrDefault(net.minecraft.core.component.DataComponents.MINIMUM_ATTACK_CHARGE, 0f));
            entry.add(attack);
            items.add(entry);
        }
        out.add("items", items);
        Files.writeString(Path.of(args[0]),
            new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
