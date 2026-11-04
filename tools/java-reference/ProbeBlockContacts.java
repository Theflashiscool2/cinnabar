// Original independent calls into the pinned native block traversal and Entity impulses.
import com.google.gson.*;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.core.BlockPos;
import net.minecraft.world.entity.*;
import net.minecraft.world.level.BlockGetter;
import net.minecraft.world.phys.*;

public class ProbeBlockContacts {
 static class TestEntity extends Entity {
 private TestEntity() {super(null,null);}
 protected void defineSynchedData(net.minecraft.network.syncher.SynchedEntityData.Builder b) {}
 public boolean hurtServer(net.minecraft.server.level.ServerLevel l,net.minecraft.world.damagesource.DamageSource s,float n) {return false;}
 protected void readAdditionalSaveData(net.minecraft.world.level.storage.ValueInput v) {}
 protected void addAdditionalSaveData(net.minecraft.world.level.storage.ValueOutput v) {}
 public MoveSimulationType getMoveSimulationType() {return MoveSimulationType.SERVER_AND_CLIENT;}
 }
 static JsonArray vec(double... values) {var a=new JsonArray();for(double v:values)a.add(v);return a;}
 public static void main(String[] args) throws Exception {
  SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
  var version=JsonParser.parseReader(new java.io.InputStreamReader(ProbeBlockContacts.class.getResourceAsStream("/version.json"),java.nio.charset.StandardCharsets.UTF_8)).getAsJsonObject();
  var out=new JsonObject();out.add("game_version",version.get("id"));out.add("protocol",version.get("protocol_version"));
  var cases=new JsonArray();
  for(double[] from:List.of(new double[]{.5,0.,.5},new double[]{-16.,-16.,-16.},new double[]{15.999999,-.000001,16.}))
   for(double[] delta:List.of(new double[]{0,0,0},new double[]{1e-6,0,0},new double[]{.2,.1,.3},new double[]{-1.2,2.1,-.7},new double[]{2.3,-1.2,1.3},new double[]{0,3.1,0},new double[]{-3.2,0,0},new double[]{1,1,1},new double[]{-1,-1,-1}))
    for(float height:new float[]{1.8f,1.5f,.6f}) {
     var f=new Vec3(from[0],from[1],from[2]);var t=f.add(delta[0],delta[1],delta[2]);
     double w=(double)(.6f/2);var box=new AABB(t.x-w,t.y,t.z-w,t.x+w,t.y+(double)height,t.z+w).deflate((double)1e-5f);
     var row=new JsonObject();row.add("from",vec(f.x,f.y,f.z));row.add("to",vec(t.x,t.y,t.z));row.add("bounds",vec(box.minX,box.minY,box.minZ,box.maxX,box.maxY,box.maxZ));
     var hits=new JsonArray();BlockGetter.forEachBlockIntersectedBetween(f,t,box,(p,step)->{hits.add(vec(p.getX(),p.getY(),p.getZ(),step));return true;});row.add("hits",hits);cases.add(row);
    }
  out.add("visits",cases);
  if(args.length==3) {
   var facts=new JsonObject();facts.addProperty("schema",1);facts.add("game_version",version.get("id"));facts.add("protocol",version.get("protocol_version"));facts.addProperty("states",net.minecraft.world.level.block.Block.BLOCK_STATE_REGISTRY.size());
   facts.addProperty("blocks_sha256",HexFormat.of().formatHex(java.security.MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[2])))));
   var bubble=new JsonArray();var damage=new JsonArray();
   for(var block:net.minecraft.core.registries.BuiltInRegistries.BLOCK) {
    var name=net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(block).toString();
    if(block instanceof net.minecraft.world.level.block.BubbleColumnBlock)bubble.add(name);
    if(block instanceof net.minecraft.world.level.block.MagmaBlock)damage.add(name);
   }
   facts.add("bubble",bubble);facts.add("server_damage",damage);Files.writeString(Path.of(args[1]),new GsonBuilder().setPrettyPrinting().create().toJson(facts)+"\n");
  }
  var uf=sun.misc.Unsafe.class.getDeclaredField("theUnsafe");uf.setAccessible(true);var unsafe=(sun.misc.Unsafe)uf.get(null);
  var e=(TestEntity)unsafe.allocateInstance(TestEntity.class);var impulses=new JsonArray();
  for(boolean surface:new boolean[]{false,true})for(boolean drag:new boolean[]{false,true})for(double y:new double[]{-2.,-.9,-.3,-.01,0.,.7,1.8,2.}) {
   e.setDeltaMovement(.125,y,-.25);e.fallDistance=4.;
   if(surface)e.onAboveBubbleColumn(drag,BlockPos.ZERO);else e.onInsideBubbleColumn(drag);
   var row=new JsonObject();row.addProperty("surface",surface);row.addProperty("drag",drag);row.addProperty("input",y);row.add("velocity",vec(e.getDeltaMovement().x,e.getDeltaMovement().y,e.getDeltaMovement().z));row.addProperty("fall",e.fallDistance);impulses.add(row);
  }
  out.add("impulses",impulses);Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
 }
}
