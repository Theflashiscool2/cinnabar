// Original reference probe; run only with the hash-verified pinned client and official libraries.
import com.google.gson.*;
import io.netty.buffer.Unpooled;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.network.protocol.game.ServerboundAttackPacket;
import net.minecraft.world.entity.Avatar;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.phys.AABB;
import net.minecraft.world.phys.Vec3;
import java.util.Map;

public final class ProbeEntityTargeting {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var result = new JsonObject(); var poses = new JsonObject();
        var field = Avatar.class.getDeclaredField("POSES"); field.setAccessible(true);
        var dimensions = (Map<Pose, EntityDimensions>) field.get(null);
        for (var pose : new Pose[]{Pose.STANDING, Pose.CROUCHING}) {
            var d = dimensions.get(pose); var row = new JsonArray();
            row.add(d.width()); row.add(d.height()); poses.add(pose.name(), row);
        }
        result.add("player_dimensions", poses);
        var packets = new JsonArray();
        for (int id : new int[]{0, 15, 127, 128, 300, -1}) {
            var row = new JsonObject(); row.addProperty("id", id);
            var bytes = new JsonArray(); var buffer = Unpooled.buffer();
            try {
                ServerboundAttackPacket.STREAM_CODEC.encode(buffer, new ServerboundAttackPacket(id));
                while (buffer.isReadable()) bytes.add(buffer.readUnsignedByte());
            } finally { buffer.release(); }
            row.add("payload", bytes); packets.add(row);
        }
        result.add("attack_packets", packets);
        var clips = new JsonArray();
        var box = new AABB(-.3, 0, 2.7, .3, 1.8, 3.3);
        for (var origin : new Vec3[]{new Vec3(0,1.62,0), new Vec3(.4,1,0), new Vec3(0,1,3), new Vec3(.3,1,0), new Vec3(.3,1,3)}) {
            var row = new JsonObject(); var from = new JsonArray();
            from.add(origin.x); from.add(origin.y); from.add(origin.z); row.add("from",from);
            row.addProperty("inside",box.contains(origin));
            var hit = box.clip(origin, origin.add(0,0,5));
            if (hit.isPresent()) row.addProperty("distance", origin.distanceTo(hit.get()));
            clips.add(row);
        }
        result.add("clips",clips);
        var corpus = new JsonArray();
        var random = new java.util.Random(777);
        for (int i=0; i<128; i++) {
            Vec3 from = i<5 ? new Vec3[]{new Vec3(0,1.62,0),new Vec3(.3,1,3),new Vec3(-.3,1,3),new Vec3(.3,1,0),new Vec3(0,1,0)}[i]
                : new Vec3(random.nextDouble()*2-1,random.nextDouble()*3-1,random.nextDouble()*6-1);
            Vec3 to = i<5 ? (i==4 ? new Vec3(0,1,2.7) : from.add(0,0,5))
                : new Vec3(random.nextDouble()*2-1,random.nextDouble()*3-1,random.nextDouble()*6-1);
            var row=new JsonObject();
            for (String key:new String[]{"from","to"}) {
                Vec3 v=key.equals("from")?from:to; var vector=new JsonArray();
                vector.add(v.x);vector.add(v.y);vector.add(v.z);row.add(key,vector);
            }
            var hit=box.clip(from,to);
            if (box.contains(from)) row.addProperty("distance",0);
            else if (hit.isPresent()) row.addProperty("distance",from.distanceTo(hit.get()));
            corpus.add(row);
        }
        result.add("corpus",corpus);
        java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), new Gson().toJson(result));
    }
}
