// Original offline codec oracle. Run only against the verified pinned26.3 server
// and its verified libraries. No client, account, connection or gameplay is simulated.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import java.nio.file.*;
import java.nio.charset.StandardCharsets;
import java.util.function.Consumer;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.*;
import net.minecraft.network.FriendlyByteBuf;
import net.minecraft.network.protocol.game.*;
import net.minecraft.world.entity.*;
import net.minecraft.world.entity.animal.pig.Pig;
import sun.misc.Unsafe;
public final class ProbeUseLifecycle {
 static JsonArray bytes(Consumer<FriendlyByteBuf> write) {
  var buffer=new FriendlyByteBuf(Unpooled.buffer());
  try {write.accept(buffer);var out=new JsonArray();while(buffer.isReadable())out.add(buffer.readUnsignedByte());return out;}
  finally {buffer.release();}
 }
 public static void main(String[] args) throws Exception {
  if(args.length!=1)throw new IllegalArgumentException("expected output path");
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var version=SharedConstants.getCurrentVersion();
  if(!version.id().equals("26.3")||version.protocolVersion()!=777)throw new IllegalStateException("wrong runtime");
  var access=Unsafe.class.getDeclaredField("theUnsafe");access.setAccessible(true);
  var unsafe=(Unsafe)access.get(null);
  // Constructor-free identity is sufficient for this fixed codec; no Level or server is started.
  var entity=(Pig)unsafe.allocateInstance(Pig.class);entity.setId(300);
  var complete=new ClientboundEntityEventPacket(entity,EntityEvent.USE_ITEM_COMPLETE);
  var release=new ServerboundPlayerActionPacket(ServerboundPlayerActionPacket.Action.RELEASE_USE_ITEM,BlockPos.ZERO,Direction.DOWN);
  var out=new JsonObject();out.addProperty("version",version.id());out.addProperty("protocol",version.protocolVersion());
  out.addProperty("complete_event",EntityEvent.USE_ITEM_COMPLETE);
  out.add("complete_entity_300",bytes(b->ClientboundEntityEventPacket.STREAM_CODEC.encode(b,complete)));
  out.add("release_use",bytes(b->ServerboundPlayerActionPacket.STREAM_CODEC.encode(b,release)));
  Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n",StandardCharsets.UTF_8);
 }
}
