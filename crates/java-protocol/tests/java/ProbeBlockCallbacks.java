// Original offline class-membership exporter for the verified target runtime.
import com.google.gson.*;
import java.nio.file.*;
import java.security.MessageDigest;
import java.util.HexFormat;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.world.level.block.*;
public final class ProbeBlockCallbacks {
 public static void main(String[] args) throws Exception {
  if(args.length!=2)throw new IllegalArgumentException("expected verified blocks report and output paths");
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var out=new JsonObject();var version=SharedConstants.getCurrentVersion();
  out.addProperty("schema",1);out.addProperty("game_version",version.id());out.addProperty("protocol",version.protocolVersion());
  out.addProperty("states",Block.BLOCK_STATE_REGISTRY.size());
  out.addProperty("blocks_sha256",HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[0])))));
  var bubble=new JsonArray();var damage=new JsonArray();var slime=new JsonArray();
  for(var block:BuiltInRegistries.BLOCK) {
   var name=BuiltInRegistries.BLOCK.getKey(block).toString();
   if(block instanceof BubbleColumnBlock)bubble.add(name);
   if(block instanceof MagmaBlock)damage.add(name);
   if(block instanceof SlimeBlock)slime.add(name);
  }
  out.addProperty("suppresses_bounce",net.minecraft.tags.BlockTags.SUPPRESSES_BOUNCE.location().toString());
  out.add("bubble",bubble);out.add("server_damage",damage);out.add("slime",slime);
  Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
 }
}
