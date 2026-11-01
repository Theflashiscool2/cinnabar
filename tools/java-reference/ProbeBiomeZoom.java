// Independent scalar oracle: asks the pinned Java 26.3 BiomeManager which quart it resolves.
// Compile/run with the verified server and its bundled libraries; no source implementation is copied.
import net.minecraft.world.level.biome.BiomeManager;
public final class ProbeBiomeZoom {
    public static void main(String[] args) {
        if (args.length>0 && args[0].equals("blend")) {
            int[] cell = new int[3];
            var manager = new BiomeManager((x,y,z)->{cell[0]=x;cell[1]=y;cell[2]=z;return null;},1234);
            for (int[] p : new int[][]{{-1,-16,-1},{-2,-8,-13},{-1,-1,-1},{-8,-4,-8}}) {
                var counts = new java.util.TreeMap<Integer,Integer>();
                for (int dz=-2;dz<=2;dz++) for (int dx=-2;dx<=2;dx++) {
                    manager.getBiome(p[0]+dx,p[1],p[2]+dz);
                    int id = cell[0]<-1 ? 7 : Math.clamp(cell[1],-4,3)<-2 ? 11 : 3;
                    counts.merge(id,1,Integer::sum);
                }
                System.out.printf("%d,%d,%d",p[0],p[1],p[2]);
                counts.forEach((id,count)->System.out.printf(",%d:%d",id,count));
                System.out.println();
            }
            return;
        }
        int[][] points = {{0,0,0},{1,1,1},{2,2,2},{3,3,3},{15,15,15},{16,16,16},
            {-1,-1,-1},{-2,-2,-2},{-16,-64,-16},{-17,-65,-17},{29,63,-32},{30000000,319,-30000000}};
        for (long seed : new long[]{0,1,-1,1234,Long.MIN_VALUE,Long.MAX_VALUE}) {
            int[] resolved = new int[3];
            var manager = new BiomeManager((x,y,z)->{resolved[0]=x;resolved[1]=y;resolved[2]=z;return null;},seed);
            for (int[] p : points) {
                manager.getBiome(p[0],p[1],p[2]);
                System.out.printf("%d,%d,%d,%d,%d,%d,%d%n",seed,p[0],p[1],p[2],resolved[0],resolved[1],resolved[2]);
            }
        }
    }
}
