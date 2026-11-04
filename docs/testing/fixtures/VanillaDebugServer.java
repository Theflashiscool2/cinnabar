/** Acceptance-only bootstrap: enable vanilla's own debugconfig administrative command.
 * Uses verified unmodified26.3 server/classes; no fabricated packets or game state.
 */
public final class VanillaDebugServer {
    public static void main(String[] args) {
        net.minecraft.SharedConstants.IS_RUNNING_IN_IDE = true;
        net.minecraft.server.Main.main(args);
    }
}
