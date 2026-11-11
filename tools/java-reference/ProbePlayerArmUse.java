// Original independent calls into the pinned 26.3 player model; no native implementation copied.
import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.client.model.HumanoidModel.ArmPose;
import net.minecraft.client.model.player.PlayerModel;
import net.minecraft.client.model.geom.ModelPart;
import net.minecraft.client.model.geom.builders.*;
import net.minecraft.client.renderer.entity.state.AvatarRenderState;
import net.minecraft.world.InteractionHand;
import net.minecraft.world.entity.HumanoidArm;
import net.minecraft.world.entity.LivingEntity;
import net.minecraft.world.entity.Pose;
import net.minecraft.world.item.component.SwingAnimation;

public final class ProbePlayerArmUse {
    private static JsonArray vector(float... values) {
        var array = new JsonArray(); for (float value : values) array.add(value); return array;
    }
    public static void main(String[] args) throws Exception {
        SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
        var rows = new JsonArray();
        for (boolean slim : new boolean[]{false,true})
        for (boolean crouch : new boolean[]{false,true})
        for (boolean left : new boolean[]{false,true})
        for (boolean mainLeft : new boolean[]{false,true})
        for (String use : new String[]{"bow","block"})
        for (boolean swing : new boolean[]{false,true}) {
            var root = LayerDefinition.create(PlayerModel.createMesh(CubeDeformation.NONE,slim),64,64).bakeRoot();
            var model = new PlayerModel(root,slim);
            var state = new AvatarRenderState();
            state.isCrouching = crouch; state.pose = crouch ? Pose.CROUCHING : Pose.STANDING;
            state.speedValue = 1; state.walkAnimationPos = 2.2f; state.walkAnimationSpeed = .4f;
            state.ageInTicks = 20.5f; state.xRot = 75; state.yRot = -65;
            state.mainArm = mainLeft ? HumanoidArm.LEFT : HumanoidArm.RIGHT;
            state.isUsingItem = true;
            state.useItemHand = left == mainLeft ? InteractionHand.MAIN_HAND : InteractionHand.OFF_HAND;
            state.rightArmPose = state.leftArmPose = ArmPose.ITEM;
            var armPose = use.equals("bow") ? ArmPose.BOW_AND_ARROW : ArmPose.BLOCK;
            if (left) state.leftArmPose = armPose; else state.rightArmPose = armPose;
            if (swing) {
                state.currentSwing = new LivingEntity.SwingDescription(InteractionHand.OFF_HAND,SwingAnimation.DEFAULT,6);
                state.swingAnimation = .35f;
            }
            model.setupAnim(state);
            var row = new JsonObject(); row.addProperty("slim",slim); row.addProperty("crouching",crouch);
            row.addProperty("left",left); row.addProperty("main_left",mainLeft); row.addProperty("use",use);
            row.addProperty("swing",swing);
            var parts = new JsonArray();
            for (String name : new String[]{"head","body","right_arm","left_arm","right_leg","left_leg"}) {
                ModelPart part = root.getChild(name);
                parts.add(vector(part.xRot,part.yRot,part.zRot,part.x,part.y,part.z));
            }
            row.add("parts",parts); rows.add(row);
        }
        Files.writeString(Path.of(args[0]),new Gson().toJson(rows)+"\n");
    }
}
