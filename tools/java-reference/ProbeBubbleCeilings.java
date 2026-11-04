// Independent native empty-context ceiling queries. No client simulation or rendering.
import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.shapes.CollisionContext;
public class ProbeBubbleCeilings {
 public static void main(String[] args) throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var version=JsonParser.parseReader(new java.io.InputStreamReader(ProbeBubbleCeilings.class.getResourceAsStream("/version.json"))).getAsJsonObject();
  var out=new JsonObject();out.add("game_version",version.get("id"));out.add("protocol",version.get("protocol_version"));var rows=new JsonArray();
  for(var state:Block.BLOCK_STATE_REGISTRY) if(state.getBlock() instanceof ScaffoldingBlock) {
   var row=new JsonObject();row.addProperty("state",Block.getId(state));row.addProperty("surface",state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,CollisionContext.empty()).isEmpty()&&state.getFluidState().isEmpty());rows.add(row);
  }
  out.add("scaffold_ceilings",rows);Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
 }
}
