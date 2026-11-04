// Original controlled-server fixture. Observes real item-use packets after native thread admission.
import java.lang.classfile.*;
import java.lang.classfile.instruction.InvokeInstruction;
import java.lang.constant.*;
import java.lang.instrument.*;
import java.nio.file.*;
import java.security.ProtectionDomain;
import java.util.function.BiConsumer;

public final class NativeAirUseFixture {
    private static Path menuRequest;
    private static String playerName;
    private static int observed, hooks;
    private static ClassFileTransformer transformer;
    private static final String LISTENER="net/minecraft/server/network/ServerGamePacketListenerImpl";
    public static void agentmain(String argument,Instrumentation inst) throws Exception {
        if(transformer!=null)throw new IllegalStateException("Already installed");
        var parts=argument.split(",",2);
        if(parts.length!=2 || !parts[1].matches("[A-Za-z0-9_]{1,16}"))throw new IllegalArgumentException("Absolute menu request,fixture name required");
        menuRequest=Path.of(parts[0]);playerName=parts[1];
        if(!menuRequest.isAbsolute())throw new IllegalArgumentException("Dedicated absolute menu request required");
        var bridge=Path.of(NativeAirUseFixture.class.getProtectionDomain().getCodeSource().getLocation().toURI()).resolveSibling("air-use-bridge.jar");
        inst.appendToBootstrapClassLoaderSearch(new java.util.jar.JarFile(bridge.toFile()));
        Class.forName("NativeAirUseBridge",true,null).getField("receiver").set(null,(BiConsumer<Object,Object>)NativeAirUseFixture::observe);
        transformer=new ClassFileTransformer() {
            public byte[] transform(ClassLoader loader,String name,Class<?> type,ProtectionDomain domain,byte[] bytes) {
                if(!name.equals(LISTENER))return null;
                var cf=ClassFile.of();hooks=0;
                var result=cf.transformClass(cf.parse(bytes),ClassTransform.transformingMethodBodies(
                    method->method.methodName().equalsString("handleUseItem"),(builder,element)->{
                        builder.with(element);
                        if(element instanceof InvokeInstruction call && call.name().equalsString("ensureRunningOnSameThread")){
                            builder.aload(0).getfield(ClassDesc.of("net.minecraft.server.network.ServerGamePacketListenerImpl"),"player",ClassDesc.of("net.minecraft.server.level.ServerPlayer"))
                                .aload(1).invokestatic(ClassDesc.of("NativeAirUseBridge"),"accept",MethodTypeDesc.of(ConstantDescs.CD_void,ConstantDescs.CD_Object,ConstantDescs.CD_Object));
                            hooks++;
                        }
                    }));
                if(hooks!=1)throw new IllegalStateException("Expected one native thread-admitted item-use hook");
                return result;
            }
        };
        inst.addTransformer(transformer,true);
        for(var type:inst.getAllLoadedClasses())if(type.getName().equals(LISTENER.replace('/','.')))inst.retransformClasses(type);
        if(hooks!=1)throw new IllegalStateException("Target listener not instrumented");
        System.out.println("NATIVE_AIR_USE_FIXTURE installed; vanilla owns item effects and menu transactions");
    }
    private static void observe(Object player,Object packet) {
        try {
            if(!playerName.equals(player.getClass().getMethod("getScoreboardName").invoke(player)))return;
            var hand=packet.getClass().getMethod("hand").invoke(packet);
            Object stack=player.getClass().getMethod("getMainHandItem").invoke(player);
            Object item=stack.getClass().getMethod("getItem").invoke(stack);
            String description=(String)item.getClass().getMethod("getDescriptionId").invoke(item);
            System.out.println("NATIVE_AIR_USE_FIXTURE observed="+(++observed)+" player="+playerName+" hand="+hand+" item="+description
                +" sequence="+packet.getClass().getMethod("sequence").invoke(packet)
                +" yaw="+packet.getClass().getMethod("yRot").invoke(packet)+" pitch="+packet.getClass().getMethod("xRot").invoke(packet));
            // A deliberate server-side menu handler, equivalent to an isolated server plugin.
            // The existing fixture calls vanilla openMenu; neither packets nor clicks are faked.
            if(hand.toString().equals("MAIN_HAND") && description.equals("item.minecraft.compass"))Files.writeString(menuRequest,"open 4");
        } catch(Throwable error){System.err.println("NATIVE_AIR_USE_FIXTURE failed: "+error);}
    }
    public static void main(String[] args) throws Exception {
        if(args.length!=3)throw new IllegalArgumentException("Usage: PID agent.jar absolute-menu-request,player");
        var vm=com.sun.tools.attach.VirtualMachine.attach(args[0]);
        try{vm.loadAgent(args[1],args[2]);}finally{vm.detach();}
    }
}
