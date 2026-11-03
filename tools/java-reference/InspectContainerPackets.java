// Independent vectors from the pinned native packet codecs, not the Rust encoder.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import it.unimi.dsi.fastutil.ints.Int2ObjectOpenHashMap;
import java.nio.file.*;
import java.util.HexFormat;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.data.registries.VanillaRegistries;
import net.minecraft.network.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.inventory.ContainerInput;
import net.minecraft.world.item.*;

public final class InspectContainerPackets {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var lookup = VanillaRegistries.createWorldLookup();
        BuiltInRegistries.DATA_COMPONENT_INITIALIZERS.build(lookup)
            .forEach(net.minecraft.core.component.DataComponentInitializers.PendingComponents::apply);
        var access = RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
        var ops = lookup.createSerializationContext(net.minecraft.util.HashOps.CRC32C_INSTANCE);
        HashedPatchMap.HashGenerator hash = component -> component.encodeValue(ops).getOrThrow().asInt();
        var out = new JsonObject();
        for (int button = 0; button < 2; button++) {
            var changed = new Int2ObjectOpenHashMap<HashedStack>();
            changed.put(button, button == 0 ? HashedStack.EMPTY
                : HashedStack.create(new ItemStack(Items.STONE, 1), hash));
            var carried = HashedStack.create(new ItemStack(Items.STONE, 8-button), hash);
            var packet = new ServerboundContainerClickPacket(7, 129, (short)button,
                (byte)button, ContainerInput.PICKUP, changed, carried);
            var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
            try {
                ServerboundContainerClickPacket.STREAM_CODEC.encode(buffer, packet);
                byte[] bytes = new byte[buffer.readableBytes()]; buffer.readBytes(bytes);
                out.addProperty(button == 0 ? "pickup_stone8" : "place_one_stone", HexFormat.of().formatHex(bytes));
            } finally { buffer.release(); }
        }
        var buffer = new RegistryFriendlyByteBuf(Unpooled.buffer(), access);
        try {
            ServerboundContainerClosePacket.STREAM_CODEC.encode(buffer, new ServerboundContainerClosePacket(129));
            byte[] bytes = new byte[buffer.readableBytes()]; buffer.readBytes(bytes);
            out.addProperty("close129", HexFormat.of().formatHex(bytes));
        } finally { buffer.release(); }
        Files.writeString(Path.of(args[0]), new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
    }
}
