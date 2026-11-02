// Scalar/wire oracle from the verified pinned vanilla server, not a Rust roundtrip.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.protocol.game.ClientboundSetPlayerTeamPacket;
import net.minecraft.network.chat.Component;
import net.minecraft.world.scores.*;
public final class ProbeTeams {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  var out=new JsonObject();
  out.addProperty("version",SharedConstants.getCurrentVersion().id());
  var visibility=new JsonArray();for(var v:Team.Visibility.values())visibility.add(v.getSerializedName());out.add("visibility",visibility);
  var colors=new JsonArray();for(var c:TeamColor.values())colors.add(c.rgb());out.add("colors",colors);
  var scores=new Scoreboard();var team=scores.addPlayerTeam("alpha");
  team.setDisplayName(Component.literal("Team A"));team.setPlayerPrefix(Component.literal("[A] "));team.setPlayerSuffix(Component.literal(" !"));
  team.setNameTagVisibility(Team.Visibility.HIDE_FOR_OTHER_TEAMS);team.setCollisionRule(Team.CollisionRule.NEVER);
  team.setColor(Optional.of(TeamColor.RED));team.setAllowFriendlyFire(false);team.setSeeFriendlyInvisibles(true);
  scores.addPlayerToTeam("Alice",team);
  var packets=new LinkedHashMap<String,ClientboundSetPlayerTeamPacket>();
  packets.put("create",ClientboundSetPlayerTeamPacket.createAddOrModifyPacket(team,true));
  packets.put("update",ClientboundSetPlayerTeamPacket.createAddOrModifyPacket(team,false));
  packets.put("remove",ClientboundSetPlayerTeamPacket.createRemovePacket(team));
  packets.put("join",ClientboundSetPlayerTeamPacket.createPlayerPacket(team,"Bob",ClientboundSetPlayerTeamPacket.Action.ADD));
  packets.put("leave",ClientboundSetPlayerTeamPacket.createPlayerPacket(team,"Bob",ClientboundSetPlayerTeamPacket.Action.REMOVE));
  var access=RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY);
  packets.forEach((name,packet)->{var b=new RegistryFriendlyByteBuf(Unpooled.buffer(),access);try{
   ClientboundSetPlayerTeamPacket.STREAM_CODEC.encode(b,packet);byte[] bytes=new byte[b.readableBytes()];b.readBytes(bytes);out.addProperty(name,HexFormat.of().formatHex(bytes));
  }finally{b.release();}});
  System.out.println(out);
 }
}
