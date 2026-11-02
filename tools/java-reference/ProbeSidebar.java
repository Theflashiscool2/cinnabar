import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.RegistryAccess;
import net.minecraft.core.registries.BuiltInRegistries;
import net.minecraft.network.RegistryFriendlyByteBuf;
import net.minecraft.network.codec.StreamCodec;
import net.minecraft.network.chat.Component;
import net.minecraft.network.chat.numbers.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.scores.*;
import net.minecraft.world.scores.criteria.ObjectiveCriteria;
import io.netty.buffer.Unpooled;
import java.util.*;
public class ProbeSidebar {
 static <T> void dump(String name,StreamCodec<? super RegistryFriendlyByteBuf,T> codec,T packet) {
  var b=new RegistryFriendlyByteBuf(Unpooled.buffer(),RegistryAccess.fromRegistryOfRegistries(BuiltInRegistries.REGISTRY));
  try {codec.encode(b,packet);byte[] bytes=new byte[b.readableBytes()];b.readBytes(bytes);System.out.println(name+"="+HexFormat.of().formatHex(bytes));}finally{b.release();}
 }
 public static void main(String[]args){
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var s=new Scoreboard();
  var o=s.addObjective("lobby",ObjectiveCriteria.DUMMY,Component.literal("Lobby"),ObjectiveCriteria.RenderType.INTEGER,false,null);
  dump("create",ClientboundSetObjectivePacket.STREAM_CODEC,new ClientboundSetObjectivePacket(o,0));
  dump("remove",ClientboundSetObjectivePacket.STREAM_CODEC,new ClientboundSetObjectivePacket(o,1));
  dump("display",ClientboundSetDisplayObjectivePacket.STREAM_CODEC,new ClientboundSetDisplayObjectivePacket(DisplaySlot.SIDEBAR,o));
  dump("unset",ClientboundSetDisplayObjectivePacket.STREAM_CODEC,new ClientboundSetDisplayObjectivePacket(DisplaySlot.SIDEBAR,null));
  dump("score",ClientboundSetScorePacket.STREAM_CODEC,new ClientboundSetScorePacket("Alice","lobby",7,Optional.empty(),Optional.empty()));
  dump("blank",ClientboundSetScorePacket.STREAM_CODEC,new ClientboundSetScorePacket("Alice","lobby",7,Optional.of(Component.literal("Wins")),Optional.of(BlankFormat.INSTANCE)));
  dump("fixed",ClientboundSetScorePacket.STREAM_CODEC,new ClientboundSetScorePacket("Alice","lobby",7,Optional.empty(),Optional.of(new FixedFormat(Component.literal("X")))));
  dump("styled",ClientboundSetScorePacket.STREAM_CODEC,new ClientboundSetScorePacket("Alice","lobby",7,Optional.empty(),Optional.of(new StyledFormat(net.minecraft.network.chat.Style.EMPTY.withColor(net.minecraft.ChatFormatting.GOLD)))));
  o.setDisplayName(Component.literal("Updated"));
  o.setNumberFormat(new FixedFormat(Component.literal("!")));
  dump("update",ClientboundSetObjectivePacket.STREAM_CODEC,new ClientboundSetObjectivePacket(o,2));
  dump("reset",ClientboundResetScorePacket.STREAM_CODEC,new ClientboundResetScorePacket("Alice", "lobby"));
  dump("reset_all",ClientboundResetScorePacket.STREAM_CODEC,new ClientboundResetScorePacket("Alice", null));
 }
}
