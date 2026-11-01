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
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        Class<?> defaults = Class.forName("net.minecraft.client.resources.DefaultPlayerSkin");
        var field = defaults.getDeclaredField("DEFAULT_SKINS");
        field.setAccessible(true);
        Object[] skins = (Object[])field.get(null);
        Object unprofiled = defaults.getMethod("getDefaultSkin").invoke(null);
        System.out.println("default-index: " + java.util.Arrays.asList(skins).indexOf(unprofiled));
        for (String value : new String[] {"00000000-0000-0000-0000-000000000000", "80000000-0000-0000-0000-000000000000", "ffffffff-0000-0000-0000-000000000000"}) {
            UUID valueId = UUID.fromString(value);
            System.out.println("uuid-index: " + value + " = " + java.util.Arrays.asList(skins).indexOf(defaults.getMethod("get", UUID.class).invoke(null, valueId)));
        }
        var endpoint = new com.mojang.authlib.services.response.discovery.Endpoint("https://example.invalid/{profileId}");
        var endpoints = new com.mojang.authlib.services.response.discovery.Endpoints(java.util.Map.of("getProfileById", endpoint));
        var empty = com.mojang.authlib.services.response.discovery.Endpoints.empty();
        var discovery = new com.mojang.authlib.services.response.discovery.Discovery("minecraft", empty, endpoints, empty, empty, empty);
        System.out.println("discovery-schema: " + new com.google.gson.Gson().toJson(new com.mojang.authlib.services.response.discovery.DiscoveryResponse("prod", "minecraft", discovery)));
        System.out.println("patch-texture-path: " + new net.minecraft.core.ClientAsset.ResourceTexture(net.minecraft.resources.Identifier.parse("minecraft:entity/player/slim/steve")).texturePath());
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
