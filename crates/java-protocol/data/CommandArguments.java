import java.util.*;
import com.google.gson.*;
public class CommandArguments {
 public static void main(String[] args) throws Exception {
  net.minecraft.SharedConstants.tryDetectVersion();net.minecraft.server.Bootstrap.bootStrap();
  var registry=net.minecraft.core.registries.BuiltInRegistries.COMMAND_ARGUMENT_TYPE;
  var field=net.minecraft.commands.synchronization.ArgumentTypeInfos.class.getDeclaredField("BY_CLASS");field.setAccessible(true);
  var classes=(Map<Class<?>,?>)field.get(null);var result=new JsonArray();
  for(var info:registry){var out=new JsonObject();out.addProperty("id",registry.getId(info));out.addProperty("name",registry.getKey(info).toString());
   var cls=classes.entrySet().stream().filter(e->e.getValue()==info).map(Map.Entry::getKey).findFirst().orElseThrow();
   out.addProperty("argument_class",cls.getName());out.addProperty("signable",net.minecraft.commands.arguments.SignedArgument.class.isAssignableFrom(cls));
   String name=info.getClass().getName();String shape;
   if(name.endsWith("SingletonArgumentInfo"))shape="none";
   else if(name.endsWith("FloatArgumentInfo"))shape="float";
   else if(name.endsWith("DoubleArgumentInfo"))shape="double";
   else if(name.endsWith("IntegerArgumentInfo"))shape="integer";
   else if(name.endsWith("LongArgumentInfo"))shape="long";
   else if(name.endsWith("StringArgumentSerializer"))shape="string";
   else if(name.endsWith("EntityArgument$Info")||name.endsWith("ScoreHolderArgument$Info"))shape="byte";
   else if(name.endsWith("TimeArgument$Info"))shape="int";
   else if(name.contains("Resource")&&name.endsWith("Argument$Info"))shape="registry";
   else throw new IllegalStateException(name);
   out.addProperty("properties",shape);
   if(shape.equals("string")){
    var modes=new JsonArray();for(var mode:com.mojang.brigadier.arguments.StringArgumentType.StringType.values())modes.add(mode.name());out.add("modes",modes);
   }
   result.add(out);
  }
  java.nio.file.Files.writeString(java.nio.file.Path.of(args[0]),new GsonBuilder().setPrettyPrinting().create().toJson(result));
 }
}
