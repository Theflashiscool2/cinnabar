// Invokes pinned native GameType.updatePlayerAbilities; no client/server simulation.
import com.google.gson.*;
import java.nio.file.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.player.Abilities;
import net.minecraft.world.level.GameType;
public final class ProbeAbilityModes {
 public static void main(String[] args)throws Exception{
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();var rows=new JsonArray();
  for(var mode:GameType.values())for(int flags=0;flags<16;flags++){
   var a=new Abilities();a.invulnerable=(flags&1)!=0;a.flying=(flags&2)!=0;a.mayfly=(flags&4)!=0;a.instabuild=(flags&8)!=0;
   a.setFlyingSpeed(-0.01337f);a.setWalkingSpeed(0.123f);mode.updatePlayerAbilities(a);
   var r=new JsonObject();r.addProperty("mode",mode.getId());r.addProperty("initial_flags",flags);
   r.addProperty("flags",(a.invulnerable?1:0)|(a.flying?2:0)|(a.mayfly?4:0)|(a.instabuild?8:0));r.addProperty("flying_speed",a.getFlyingSpeed());r.addProperty("walking_speed",a.getWalkingSpeed());rows.add(r);
  }
  Files.writeString(Path.of(args[0]),new Gson().toJson(rows)+"\n");
 }
}
