import net.minecraft.SharedConstants;
import net.minecraft.server.Bootstrap;
import net.minecraft.world.level.biome.BiomeSpecialEffects.GrassColorModifier;
public class ProbeGrass {
 public static void main(String[] args) {
  SharedConstants.tryDetectVersion(); Bootstrap.bootStrap();
  StringBuilder s=new StringBuilder("["); boolean first=true;
  int[] edges={-30000000,-1025,-17,-16,-1,0,1,15,16,1024,30000000};
  for(int x:edges) for(int z:edges) {
   if(!first)s.append(',');first=false;
   s.append('[').append(x).append(',').append(z).append(',').append(GrassColorModifier.SWAMP.modifyColor(x,z,0)&0xffffff).append(',').append(GrassColorModifier.DARK_FOREST.modifyColor(x,z,0x79c05a)&0xffffff).append(']');
  }
  s.append(']'); System.out.println("ORACLE="+s);
  s=new StringBuilder("[");first=true;
  for(int x=-16;x<0;x++) for(int z=-16;z<0;z++) {
   int low=0;
   for(int dz=-2;dz<=2;dz++) for(int dx=-2;dx<=2;dx++)
    if((GrassColorModifier.SWAMP.modifyColor(x+dx,z+dz,0)&0xffffff)==0x4c763c) low++;
   if(!first)s.append(',');first=false;
   s.append('[').append(x).append(',').append(z).append(',').append(low).append(',').append(25-low).append(']');
  }
  System.out.println("BLEND="+s.append(']'));
 }
}
