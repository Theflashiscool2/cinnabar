import io.netty.buffer.*;
import net.minecraft.network.protocol.game.ServerboundInteractPacket;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.phys.Vec3;
import com.google.gson.*;
public final class ProbeEntityInteract {
    public static void main(String[] args) throws Exception {
        double[][] cases = {{0,0,0},{0,1.62,-0.3},{-0.25,0.5,0.5},{0.449999988,0.7,-0.449999988},{1e-9,0,0},{-4,2,0},{0,2.9,-0.3}};
        JsonArray result = new JsonArray();
        for (int i=0;i<cases.length;i++) {
            double[] v=cases[i]; boolean secondary=(i%2)==1; int id=i==0?300:42;
            InteractionHand hand=i%2==0?InteractionHand.MAIN_HAND:InteractionHand.OFF_HAND;
            ByteBuf buffer=Unpooled.buffer();
            ServerboundInteractPacket.STREAM_CODEC.encode(buffer,new ServerboundInteractPacket(id,hand,new Vec3(v[0],v[1],v[2]),secondary));
            JsonObject row=new JsonObject();row.addProperty("entity",id);row.addProperty("hand",hand.name());row.addProperty("secondary",secondary);
            JsonArray point=new JsonArray();for(double c:v)point.add(c);row.add("location",point);
            row.addProperty("payload_hex",ByteBufUtil.hexDump(buffer));result.add(row);buffer.release();
        }
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result)+"\n");
    }
}
