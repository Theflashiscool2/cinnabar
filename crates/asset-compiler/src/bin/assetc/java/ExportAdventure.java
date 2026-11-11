// Original offline permission-property facts exporter for the verified protocol 777 runtime.
import com.google.gson.*;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.resources.Identifier;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.Block;
import net.minecraft.world.level.block.state.properties.*;
public final class ExportAdventure {
    @SuppressWarnings({"rawtypes", "unchecked"})
    public static void main(String[] args) throws Exception {
        if (args.length != 2) throw new IllegalArgumentException("output.json exporter.java");
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var root = new JsonObject(); root.addProperty("schema", 1);
        root.addProperty("version", SharedConstants.getCurrentVersion().id());
        root.addProperty("protocol", SharedConstants.getCurrentVersion().protocolVersion());
        root.addProperty("source_sha256", HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[1])))));
        root.addProperty("state_count", Block.BLOCK_STATE_REGISTRY.size());
        var blocks = new JsonArray();
        for (var block : BuiltInRegistries.BLOCK) {
            if (BuiltInRegistries.BLOCK.getId(block) != blocks.size()) throw new IllegalStateException("block ID order");
            var row = new JsonObject(); row.addProperty("name", BuiltInRegistries.BLOCK.getKey(block).toString());
            boolean entity = block.defaultBlockState().hasBlockEntity();
            for (var state : block.getStateDefinition().getPossibleStates())
                if (state.hasBlockEntity() != entity) throw new IllegalStateException("state-dependent block entity type");
            row.addProperty("block_entity", entity);
            var properties = new JsonObject();
            for (Property property : block.getStateDefinition().getProperties()) {
                var entry = new JsonObject();
                String kind = property instanceof IntegerProperty ? "integer" : property instanceof BooleanProperty ? "boolean" : property instanceof EnumProperty ? "enum" : null;
                if (kind == null) throw new IllegalStateException("unknown property class " + property.getClass());
                entry.addProperty("kind", kind); entry.addProperty("class", property.getClass().getName());
                var values = new ArrayList<Comparable>(property.getPossibleValues());
                values.sort((a,b) -> a.compareTo(b));
                var order = new JsonArray();
                for (var value : values) order.add(property.getName(value));
                entry.add("values", order); properties.add(property.getName(), entry);
            }
            row.add("properties", properties); blocks.add(row);
        }
        root.add("blocks", blocks);
        // IntegerProperty.getValue delegates to Integer.parseInt, whose digit lookup is
        // Character.digit(char,10) over UTF-16 code units, including BMP Unicode digits.
        var digitStarts = new JsonArray();
        for (int c=0; c<=Character.MAX_VALUE; c++) {
            int digit = Character.digit((char)c, 10);
            if (digit < 0) continue;
            if (digit != 0) throw new IllegalStateException("non-contiguous decimal digits");
            for (int i=0; i<10; i++)
                if (Character.digit((char)(c+i),10) != i) throw new IllegalStateException("decimal digit range");
            digitStarts.add(c); c+=9;
        }
        root.add("integer_digit_starts", digitStarts);
        root.addProperty("default_namespace", Identifier.DEFAULT_NAMESPACE);
        var namespace = new StringBuilder(); var path = new StringBuilder();
        for (int c=0; c<128; c++) {
            if (Identifier.isValidNamespace(String.valueOf((char)c))) namespace.append((char)c);
            if (Identifier.validPathChar((char)c)) path.append((char)c);
        }
        root.addProperty("namespace_characters", namespace.toString());
        root.addProperty("path_characters", path.toString());
        Files.writeString(Path.of(args[0]), new GsonBuilder().disableHtmlEscaping().create().toJson(root)+"\n", StandardCharsets.UTF_8);
    }
}
