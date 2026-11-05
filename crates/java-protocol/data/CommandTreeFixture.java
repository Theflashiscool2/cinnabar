import com.mojang.brigadier.*;
import com.mojang.brigadier.builder.*;
import com.mojang.brigadier.tree.*;
import net.minecraft.commands.arguments.*;
import net.minecraft.network.*;
import net.minecraft.network.protocol.game.*;
import net.minecraft.network.chat.*;
import net.minecraft.resources.Identifier;
import io.netty.buffer.Unpooled;
import java.util.*;
public class CommandTreeFixture {
 public static void main(String[] args) throws Exception {
  net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
  var d=new CommandDispatcher<Object>();
  var msg=d.register(LiteralArgumentBuilder.<Object>literal("msg").then(RequiredArgumentBuilder.<Object,Object>argument("targets",(com.mojang.brigadier.arguments.ArgumentType)EntityArgument.players()).then(RequiredArgumentBuilder.<Object,Object>argument("message",(com.mojang.brigadier.arguments.ArgumentType)MessageArgument.message()).executes(c->1))));
  d.register(LiteralArgumentBuilder.<Object>literal("w").redirect(msg));
  d.register(LiteralArgumentBuilder.<Object>literal("execute").then(LiteralArgumentBuilder.<Object>literal("run").redirect(d.getRoot())));
  d.register(LiteralArgumentBuilder.<Object>literal("spawn").executes(c->1));
  var packet=new ClientboundCommandsPacket(d.getRoot(),new ClientboundCommandsPacket.NodeInspector<Object>(){
   public Identifier suggestionId(ArgumentCommandNode<Object,?> node){return null;}
   public boolean isExecutable(CommandNode<Object> node){return node.getCommand()!=null;}
   public boolean isRestricted(CommandNode<Object> node){return false;}
  });
  var b=new FriendlyByteBuf(Unpooled.buffer());ClientboundCommandsPacket.STREAM_CODEC.encode(b,packet);byte[] bytes=new byte[b.readableBytes()];b.readBytes(bytes);
  java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]),HexFormat.of().formatHex(bytes));
  for(String text:List.of("msg Bob hello 夜","w Bob hi there","execute run msg Bob nested","msg Bob ","msg Bob hello @a[bad","spawn","unknown words")){
   var parse=d.parse(text,new Object());var a=SignableCommand.of(parse).arguments();System.out.println(text+" => "+a.stream().map(v->v.name()+"="+v.value()).toList());
  }
  var r=new FriendlyByteBuf(Unpooled.buffer());ServerboundChatCommandSignedPacket.STREAM_CODEC.encode(r,new ServerboundChatCommandSignedPacket("msg Bob hello",java.time.Instant.ofEpochMilli(1700000000123L),42,ArgumentSignatures.EMPTY,new LastSeenMessages.Update(0,new BitSet(),(byte)1)));bytes=new byte[r.readableBytes()];r.readBytes(bytes);System.out.println("signed-empty="+HexFormat.of().formatHex(bytes));
 }
}
