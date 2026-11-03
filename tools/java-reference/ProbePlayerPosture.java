// Original reference probe: verified Java 26.3 model API, no game implementation copied.
import com.google.gson.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.client.model.player.PlayerModel;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.model.geom.builders.*;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.world.entity.Pose;

public final class ProbePlayerPosture {
    private static JsonArray values(float... xs) {
        var out = new JsonArray(); for (float x : xs) out.add(x); return out;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var output = new JsonArray();
        for (boolean slim : new boolean[]{false, true})
        for (boolean crouching : new boolean[]{false, true})
        for (float[] sample : new float[][]{{0,0,0},{2.2f,.4f,20.5f},{5.5f,.8f,80.25f}}) {
            ModelPart root = LayerDefinition.create(PlayerModel.createMesh(CubeDeformation.NONE, slim),64,64).bakeRoot();
            var model = new PlayerModel(root, slim);
            var state = new AvatarRenderState();
            state.isCrouching = crouching;
            state.pose = crouching ? Pose.CROUCHING : Pose.STANDING;
            state.speedValue = 1;
            state.walkAnimationPos = sample[0]; state.walkAnimationSpeed = sample[1];
            state.ageInTicks = sample[2]; state.xRot = 15; state.yRot = -25;
            state.showHat=state.showJacket=state.showLeftPants=state.showRightPants=true;
            state.showLeftSleeve=state.showRightSleeve=true;
            model.setupAnim(state);
            var row = new JsonObject(); row.addProperty("slim",slim); row.addProperty("crouching",crouching);
            row.add("sample",values(sample)); row.add("root_position",values(root.x,root.y,root.z)); row.add("root_rotation",values(root.xRot,root.yRot,root.zRot)); row.add("root_scale",values(root.xScale,root.yScale,root.zScale)); var parts = new JsonObject();
            for (String name : new String[]{"head","body","right_arm","left_arm","right_leg","left_leg"}) {
                ModelPart part = root.getChild(name); var p = new JsonObject();
                p.add("position",values(part.x,part.y,part.z));
                p.add("rotation",values(part.xRot,part.yRot,part.zRot)); parts.add(name,p);
            }
            row.add("parts",parts); output.add(row);
        }
        String json = new Gson().toJson(output);
        if (args.length == 1) java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]), json);
        else System.out.println(json);
    }
}
