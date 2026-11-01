// Original offline exporter of the pinned runtime's model and entity-schema data.
// No official client process, network connection, or renderer is started.
import com.google.gson.*;
import java.lang.reflect.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.entity.EntityType;
import net.minecraft.world.entity.Pose;
import net.minecraft.network.syncher.EntityDataSerializers;
import net.minecraft.network.syncher.EntityDataAccessor;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.model.geom.builders.*;
import net.minecraft.client.model.animal.pig.*;
import net.minecraft.client.model.player.PlayerModel;

public final class ExportEntities {
    static final Gson JSON = new GsonBuilder().setPrettyPrinting().disableHtmlEscaping().create();
    static Object field(Object object, String name) throws Exception {
        Field f = object.getClass().getDeclaredField(name);
        f.setAccessible(true);
        return f.get(object);
    }
    static JsonArray floats(float... values) {
        JsonArray a = new JsonArray();
        for (float n : values) {
            if (!Float.isFinite(n)) throw new IllegalArgumentException("nonfinite model data");
            a.add(n);
        }
        return a;
    }
    static JsonObject pistonBlocks() {
        JsonArray runs = new JsonArray();
        JsonArray variants = new JsonArray();
        JsonObject previous = null;
        int count = 0;
        for (var state : net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY) {
            int id = net.minecraft.world.level.block.Block.getId(state);
            if (id != count++) throw new IllegalStateException("noncontiguous block state registry");
            boolean base = state.is(net.minecraft.world.level.block.Blocks.PISTON)
                || state.is(net.minecraft.world.level.block.Blocks.STICKY_PISTON);
            boolean head = state.is(net.minecraft.world.level.block.Blocks.PISTON_HEAD);
            boolean moving = state.is(net.minecraft.world.level.block.Blocks.MOVING_PISTON);
            if (base || head || moving) {
                JsonObject variant = new JsonObject();
                variant.addProperty("id", id);
                variant.addProperty("block", BuiltInRegistries.BLOCK.getId(state.getBlock()));
                variant.addProperty("kind", base ? "base" : head ? "head" : "moving");
                variant.addProperty("facing", state.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.FACING).get3DDataValue());
                variant.addProperty("sticky", base ? state.is(net.minecraft.world.level.block.Blocks.STICKY_PISTON)
                    : state.getValue(net.minecraft.world.level.block.state.properties.BlockStateProperties.PISTON_TYPE) == net.minecraft.world.level.block.state.properties.PistonType.STICKY);
                variant.addProperty("extended", base && state.getValue(net.minecraft.world.level.block.piston.PistonBaseBlock.EXTENDED));
                variant.addProperty("short", head && state.getValue(net.minecraft.world.level.block.piston.PistonHeadBlock.SHORT));
                variants.add(variant);
            }
            JsonObject fact = new JsonObject();
            fact.addProperty("reaction", state.getPistonPushReaction().name());
            fact.addProperty("air", state.isAir());
            fact.addProperty("block_entity", state.hasBlockEntity());
            fact.addProperty("piston_base", base);
            fact.addProperty("extended", base && state.getValue(net.minecraft.world.level.block.piston.PistonBaseBlock.EXTENDED));
            fact.addProperty("unbreakable", state.getDestroySpeed(net.minecraft.world.level.EmptyBlockGetter.INSTANCE, net.minecraft.core.BlockPos.ZERO) == -1);
            fact.addProperty("sticky", state.is(net.minecraft.world.level.block.Blocks.SLIME_BLOCK) ? "slime"
                : state.is(net.minecraft.world.level.block.Blocks.HONEY_BLOCK) ? "honey" : "none");
            if (!fact.equals(previous)) {
                var run = fact.deepCopy();
                run.addProperty("first", id);
                runs.add(run);
                previous = fact;
            }
        }
        JsonObject out = new JsonObject();
        out.addProperty("states", count);
        out.addProperty("max_push_depth", net.minecraft.world.level.block.piston.PistonStructureResolver.MAX_PUSH_DEPTH);
        out.add("runs", runs);
        out.add("variants", variants);
        out.addProperty("air", net.minecraft.world.level.block.Block.getId(net.minecraft.world.level.block.Blocks.AIR.defaultBlockState()));
        return out;
    }
    @SuppressWarnings("unchecked")
    static JsonObject part(ModelPart part, String name) throws Exception {
        JsonObject out = new JsonObject();
        out.addProperty("name", name);
        out.add("offset", floats(part.x, part.y, part.z));
        out.add("rotation", floats(part.xRot, part.yRot, part.zRot));
        out.add("scale", floats(part.xScale, part.yScale, part.zScale));
        out.addProperty("visible", part.visible);
        out.addProperty("skip_draw", part.skipDraw);
        JsonArray faces = new JsonArray();
        for (Object cube : (List<?>)field(part, "cubes")) {
            for (Object polygon : (Object[])field(cube, "polygons")) {
                JsonObject face = new JsonObject();
                var normal = (org.joml.Vector3fc)field(polygon, "normal");
                face.add("normal", floats(normal.x(), normal.y(), normal.z()));
                JsonArray vertices = new JsonArray();
                for (Object v : (Object[])field(polygon, "vertices")) {
                    // The factory stores pixel coordinates; Rust performs the model/world transform.
                    var vertex = (ModelPart.Vertex)v;
                    vertices.add(floats(vertex.x(), vertex.y(), vertex.z(), vertex.u(), vertex.v()));
                }
                face.add("vertices", vertices);
                faces.add(face);
            }
        }
        out.add("faces", faces);
        JsonArray children = new JsonArray();
        for (var e : new TreeMap<>((Map<String, ModelPart>)field(part, "children")).entrySet()) {
            children.add(part(e.getValue(), e.getKey()));
        }
        out.add("children", children);
        return out;
    }
    static JsonArray metadata(Class<?> leaf) throws Exception {
        TreeMap<Integer, JsonObject> entries = new TreeMap<>();
        for (Class<?> cls = leaf; cls != null; cls = cls.getSuperclass()) {
            for (Field f : cls.getDeclaredFields()) {
                if (!Modifier.isStatic(f.getModifiers()) || f.getType() != EntityDataAccessor.class) continue;
                f.setAccessible(true);
                var accessor = (EntityDataAccessor<?>)f.get(null);
                JsonObject entry = new JsonObject();
                entry.addProperty("owner", cls.getName());
                entry.addProperty("name", f.getName());
                entry.addProperty("index", accessor.id());
                entry.addProperty("serializer", EntityDataSerializers.getSerializedId(accessor.serializer()));
                if (entries.put(accessor.id(), entry) != null) throw new IllegalStateException("duplicate metadata index");
            }
        }
        JsonArray out = new JsonArray();
        entries.values().forEach(out::add);
        return out;
    }
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("expected models and entity-facts output paths");
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        JsonObject models = new JsonObject();
        models.add("pig", part(PigModel.createBodyLayer(CubeDeformation.NONE).bakeRoot(), "root"));
        models.add("cold_pig", part(ColdPigModel.createBodyLayer(CubeDeformation.NONE).bakeRoot(), "root"));
        models.add("baby_pig", part(BabyPigModel.createBodyLayer(CubeDeformation.NONE).bakeRoot(), "root"));
        models.add("player_wide", part(LayerDefinition.create(PlayerModel.createMesh(CubeDeformation.NONE, false), 64, 64).bakeRoot(), "root"));
        models.add("player_slim", part(LayerDefinition.create(PlayerModel.createMesh(CubeDeformation.NONE, true), 64, 64).bakeRoot(), "root"));
        models.add("statue_standing", part(net.minecraft.client.model.animal.golem.CopperGolemModel.createBodyLayer().bakeRoot(), "root"));
        models.add("statue_running", part(net.minecraft.client.model.animal.golem.CopperGolemModel.createRunningPoseBodyLayer().bakeRoot(), "root"));
        models.add("statue_sitting", part(net.minecraft.client.model.animal.golem.CopperGolemModel.createSittingPoseBodyLayer().bakeRoot(), "root"));
        models.add("statue_star", part(net.minecraft.client.model.animal.golem.CopperGolemModel.createStarPoseBodyLayer().bakeRoot(), "root"));
        models.add("conduit_shell", part(net.minecraft.client.renderer.blockentity.ConduitRenderer.createShellLayer().bakeRoot(), "root"));
        models.add("conduit_cage", part(net.minecraft.client.renderer.blockentity.ConduitRenderer.createCageLayer().bakeRoot(), "root"));
        models.add("conduit_wind", part(net.minecraft.client.renderer.blockentity.ConduitRenderer.createWindLayer().bakeRoot(), "root"));
        models.add("conduit_eye", part(net.minecraft.client.renderer.blockentity.ConduitRenderer.createEyeLayer().bakeRoot(), "root"));
        models.add("pot_base", part(net.minecraft.client.renderer.blockentity.DecoratedPotRenderer.createBaseLayer().bakeRoot(), "root"));
        models.add("pot_sides", part(net.minecraft.client.renderer.blockentity.DecoratedPotRenderer.createSidesLayer().bakeRoot(), "root"));
        Files.writeString(Path.of(args[0]), JSON.toJson(models)+"\n");
        JsonObject facts = new JsonObject();
        JsonArray types = new JsonArray();
        for (EntityType<?> type : BuiltInRegistries.ENTITY_TYPE) {
            JsonObject entry = new JsonObject();
            entry.addProperty("id", BuiltInRegistries.ENTITY_TYPE.getId(type));
            entry.addProperty("name", EntityType.getKey(type).toString());
            entry.addProperty("update_interval", type.updateInterval());
            var dim = type.getDimensions();
            entry.add("dimensions", floats(dim.width(), dim.height(), dim.eyeHeight()));
            types.add(entry);
        }
        facts.add("types", types);
        JsonObject metadata = new JsonObject();
        metadata.add("pig", metadata(net.minecraft.world.entity.animal.pig.Pig.class));
        metadata.add("player", metadata(net.minecraft.world.entity.player.Player.class));
        metadata.add("item", metadata(net.minecraft.world.entity.item.ItemEntity.class));
        facts.add("metadata", metadata);
        JsonArray poses = new JsonArray();
        for (Pose pose : Pose.values()) poses.add(pose.name().toLowerCase(Locale.ROOT));
        facts.add("poses", poses);
        facts.addProperty("pig_default_variant", net.minecraft.world.entity.animal.pig.PigVariants.DEFAULT.identifier().toString());
        Field defaults = net.minecraft.client.resources.DefaultPlayerSkin.class.getDeclaredField("DEFAULT_SKINS");
        defaults.setAccessible(true);
        JsonArray skins = new JsonArray();
        for (var skin : (net.minecraft.world.entity.player.PlayerSkin[])defaults.get(null)) {
            JsonObject entry = new JsonObject();
            entry.addProperty("texture", skin.body().texturePath().toString());
            entry.addProperty("model", skin.model().getSerializedName());
            skins.add(entry);
        }
        facts.add("default_player_skins", skins);
        facts.addProperty("default_player_skin_index", java.util.Arrays.asList(
            (net.minecraft.world.entity.player.PlayerSkin[])defaults.get(null)
        ).indexOf(net.minecraft.client.resources.DefaultPlayerSkin.getDefaultSkin()));
        JsonObject clothing = new JsonObject();
        for (var part : net.minecraft.world.entity.player.PlayerModelPart.values()) {
            clothing.addProperty(part.getId(), part.getMask());
        }
        facts.add("player_model_parts", clothing);
        facts.add("piston_blocks", pistonBlocks());
        Files.writeString(Path.of(args[1]), JSON.toJson(facts)+"\n");
    }
}
