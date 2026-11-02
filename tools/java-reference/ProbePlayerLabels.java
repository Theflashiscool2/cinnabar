import com.google.gson.JsonObject;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.Avatar;
import net.minecraft.world.entity.EntityAttachment;
import net.minecraft.world.entity.EntityDimensions;
import net.minecraft.world.entity.Pose;
import java.util.List;
import java.util.Map;

/** Original API/reflection probe; no native implementation or game resources are copied. */
public class ProbePlayerLabels {
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion();
        Bootstrap.bootStrap();
        var field = Avatar.class.getDeclaredField("POSES");
        field.setAccessible(true);
        if (!(field.get(null) instanceof Map<?, ?> poses)) {
            throw new IllegalStateException("Native avatar pose map changed");
        }
        var out = new JsonObject();
        out.addProperty("version", SharedConstants.getCurrentVersion().id());
        for (var pose : List.of(Pose.STANDING, Pose.CROUCHING)) {
            if (!(poses.get(pose) instanceof EntityDimensions dimensions)) {
                throw new IllegalStateException("Missing native pose " + pose);
            }
            var attachment = dimensions.attachments().getNullable(EntityAttachment.NAME_TAG, 0, 0f);
            var row = new JsonObject();
            row.addProperty("height", dimensions.height());
            row.addProperty("name_tag_y", attachment.y);
            out.add(pose.toString(), row);
        }
        System.out.println(out);
    }
}
