// Original calls into the verified native block registry and collision-context implementation.
import com.google.gson.*;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.nio.file.*;
import java.util.*;
import net.minecraft.SharedConstants;
import net.minecraft.core.*;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.entity.Entity;
import net.minecraft.world.item.ItemStack;
import net.minecraft.world.level.EmptyBlockGetter;
import net.minecraft.world.level.block.*;
import net.minecraft.world.phys.shapes.*;

public final class ExportClimbingFacts {
    static JsonArray boxes(VoxelShape shape) {
        var out=new JsonArray();
        for(var b:shape.toAabbs()) {
            var row=new JsonArray();
            for(double value:new double[]{b.minX,b.minY,b.minZ,b.maxX,b.maxY,b.maxZ}) row.add(value);
            out.add(row);
        }
        return out;
    }
    static EntityCollisionContext context(boolean descending,double bottom) throws Exception {
        var c=EntityCollisionContext.class.getDeclaredConstructor(boolean.class,boolean.class,double.class,ItemStack.class,boolean.class,Entity.class);
        c.setAccessible(true);
        return c.newInstance(descending,false,bottom,ItemStack.EMPTY,false,null);
    }
    public static void main(String[] args) throws Exception {
        if(args.length!=3) throw new IllegalArgumentException("OUTPUT BLOCK_REPORT FIXTURE_OUTPUT");
        SharedConstants.tryDetectVersion();Bootstrap.bootStrap();
        var version=JsonParser.parseReader(new InputStreamReader(ExportClimbingFacts.class.getResourceAsStream("/version.json"),StandardCharsets.UTF_8)).getAsJsonObject();
        var out=new JsonObject();out.addProperty("schema",1);out.add("game_version",version.get("id"));out.add("protocol",version.get("protocol_version"));
        out.addProperty("blocks_sha256",HexFormat.of().formatHex(java.security.MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(args[1])))));
        String previous=null;int count=0,total=0;var runs=new JsonArray();var classes=new TreeMap<String,Set<String>>();
        for(var state:Block.BLOCK_STATE_REGISTRY) {
            var b=state.getBlock();
            String kind=b instanceof TrapDoorBlock?"trapdoor":b instanceof LadderBlock?"ladder":b instanceof ScaffoldingBlock?"scaffolding":"ordinary";
            if(!kind.equals("ordinary")) classes.computeIfAbsent(kind,k->new TreeSet<>()).add(net.minecraft.core.registries.BuiltInRegistries.BLOCK.getKey(b).toString());
            if(!kind.equals(previous)&&count>0) {var row=new JsonArray();row.add(count);row.add(previous);runs.add(row);count=0;}
            previous=kind;count++;total++;
        }
        var last=new JsonArray();last.add(count);last.add(previous);runs.add(last);out.addProperty("states",total);out.add("runs",runs);
        var block=Blocks.SCAFFOLDING.defaultBlockState();
        out.add("stable",boxes(block.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,context(false,1.))));
        out.add("unstable",boxes(block.setValue(ScaffoldingBlock.DISTANCE,1).setValue(ScaffoldingBlock.BOTTOM,true).getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,context(true,0.))));
        var below=ScaffoldingBlock.class.getDeclaredField("SHAPE_BELOW_BLOCK");below.setAccessible(true);
        out.addProperty("below_max_y",((VoxelShape)below.get(null)).max(Direction.Axis.Y));
        Files.writeString(Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(out)+"\n");
        var fixtures=new JsonObject();fixtures.add("classes",new Gson().toJsonTree(classes));var cases=new JsonArray();
        for(int distance:new int[]{0,1,7}) for(boolean bottom:new boolean[]{false,true}) for(boolean descending:new boolean[]{false,true})
            for(double height:new double[]{-.01,0.,.125,.99998,.999991,1.,1.01}) {
                var state=block.setValue(ScaffoldingBlock.DISTANCE,distance).setValue(ScaffoldingBlock.BOTTOM,bottom);
                var row=new JsonObject();row.addProperty("state",Block.getId(state));row.addProperty("bottom",height);row.addProperty("descending",descending);
                row.add("boxes",boxes(state.getCollisionShape(EmptyBlockGetter.INSTANCE,BlockPos.ZERO,context(descending,height))));cases.add(row);
            }
        fixtures.add("scaffolding",cases);
        Files.writeString(Path.of(args[2]),new GsonBuilder().setPrettyPrinting().create().toJson(fixtures)+"\n");
    }
}
