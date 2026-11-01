// Original codec probe. Run only against the verified pinned vanilla runtime.
import java.nio.charset.StandardCharsets;
import java.util.Base64;
import java.util.UUID;
import com.google.common.collect.ImmutableMultimap;
import com.mojang.authlib.GameProfile;
import com.mojang.authlib.properties.Property;
import com.mojang.authlib.properties.PropertyMap;
import net.minecraft.SharedConstants;
import net.minecraft.nbt.NbtOps;
import net.minecraft.nbt.StringTag;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.item.component.ResolvableProfile;

public final class InspectHeadProfile {
    private static void show(String label, ResolvableProfile profile) {
        System.out.println(label + ": " + ResolvableProfile.CODEC.encodeStart(NbtOps.INSTANCE, profile).getOrThrow());
    }
    public static void main(String[] args) {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        UUID id = UUID.fromString("12345678-1234-5678-9abc-def012345678");
        show("name", ResolvableProfile.createUnresolved("Fixture"));
        show("compact-name-decoded", ResolvableProfile.CODEC.parse(NbtOps.INSTANCE, StringTag.valueOf("Fixture")).getOrThrow());
        show("uuid", ResolvableProfile.createUnresolved(id));
        show("resolved", ResolvableProfile.createResolved(new GameProfile(id, "Fixture")));
        String texture = Base64.getEncoder().encodeToString("{\"textures\":{\"SKIN\":{\"url\":\"https://textures.minecraft.net/texture/0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef\"}}}".getBytes(StandardCharsets.UTF_8));
        Property property = new Property("textures", texture, "fixture-signature");
        show("texture", ResolvableProfile.createResolved(new GameProfile(id, "Fixture", new PropertyMap(ImmutableMultimap.of("textures", property)))));
    }
}
