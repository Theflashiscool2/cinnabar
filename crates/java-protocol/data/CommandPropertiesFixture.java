import net.minecraft.network.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.commands.synchronization.ArgumentTypeInfo;
import io.netty.buffer.Unpooled;
import java.util.*;
public class CommandPropertiesFixture {
 public static void main(String[] args) throws Exception {
  net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
  var registry=net.minecraft.core.registries.BuiltInRegistries.COMMAND_ARGUMENT_TYPE;
  var out=new FriendlyByteBuf(Unpooled.buffer());out.writeVarInt(registry.size()+1);out.writeByte(0);out.writeVarInt(registry.size());for(int i=0;i<registry.size();i++)out.writeVarInt(i+1);
  for(var raw:registry){
   ArgumentTypeInfo info=raw;String name=raw.getClass().getName();var p=new FriendlyByteBuf(Unpooled.buffer());
   if(name.endsWith("SingletonArgumentInfo")){}
   else if(name.endsWith("FloatArgumentInfo")){p.writeByte(3);p.writeFloat(-1.5f);p.writeFloat(2.25f);}
   else if(name.endsWith("DoubleArgumentInfo")){p.writeByte(3);p.writeDouble(-1.5);p.writeDouble(2.25);}
   else if(name.endsWith("IntegerArgumentInfo")){p.writeByte(3);p.writeInt(-7);p.writeInt(42);}
   else if(name.endsWith("LongArgumentInfo")){p.writeByte(3);p.writeLong(-9007199254740993L);p.writeLong(9007199254740993L);}
   else if(name.endsWith("StringArgumentSerializer"))p.writeVarInt(1);
   else if(name.endsWith("EntityArgument$Info")||name.endsWith("ScoreHolderArgument$Info"))p.writeByte(3);
   else if(name.endsWith("TimeArgument$Info"))p.writeInt(1234);
   else if(name.contains("Resource")&&name.endsWith("Argument$Info"))p.writeUtf("minecraft:worldgen/biome");
   else throw new IllegalStateException(name);
   var template=info.deserializeFromNetwork(p);if(p.isReadable())throw new IllegalStateException("Unread properties: "+name);
   out.writeByte(22);out.writeVarInt(0);out.writeUtf("arg_"+registry.getId(raw));out.writeVarInt(registry.getId(raw));info.serializeToNetwork(template,out);out.writeUtf("minecraft:ask_server");
  }
  out.writeVarInt(0);byte[] bytes=new byte[out.readableBytes()];out.getBytes(0,bytes);
  ClientboundCommandsPacket.STREAM_CODEC.decode(out);if(out.isReadable())throw new IllegalStateException("Unread tree bytes");
  java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]),HexFormat.of().formatHex(bytes));System.out.println("Native validated "+(registry.size()+1)+" nodes and all parser property shapes");
 }
}
