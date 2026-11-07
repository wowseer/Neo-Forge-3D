package clash.forgebridge;

import java.util.ArrayList;
import java.util.List;

import com.google.gson.JsonObject;

/**
 * Lage des Tisch-Fensters für die Fensterübergabe zwischen Gastgeber und Tisch: Der Tisch öffnet
 * sich dort, wo das Fenster des Gastgebers war ({@link TableLauncher.Config#window}), und meldet
 * seine Lage laufend zurück (Befehl {@code window}, {@link ClashBridge#tableWindow()}), damit der
 * Gastgeber nach der Partie wieder genau dort erscheint.
 *
 * <p>Maße in Bildschirmpixeln (physisch, nicht skaliert) und für den <b>Inhaltsbereich</b> des
 * Fensters (ohne Titelleiste und Rahmen).
 *
 * @param x      linke Kante des Inhaltsbereichs
 * @param y      obere Kante des Inhaltsbereichs
 * @param width  Breite des Inhaltsbereichs
 * @param height Höhe des Inhaltsbereichs
 * @param mode   Fenstermodus (bei {@link Mode#MAXIMIZED} und {@link Mode#FULLSCREEN} sind die Maße
 *               die des nicht maximierten Fensters, soweit bekannt)
 */
public record TableWindow(int x, int y, int width, int height, Mode mode) {
    /** Fenstermodus. */
    public enum Mode {
        WINDOWED, MAXIMIZED, FULLSCREEN;

        /** Name im Protokoll ({@code "windowed"}, {@code "maximized"}, {@code "fullscreen"}). */
        public String wire() {
            return name().toLowerCase(java.util.Locale.ROOT);
        }
    }

    /**
     * Startargumente für Godot ({@code --position}, {@code --resolution}, dazu {@code --maximized}
     * bzw. {@code --fullscreen}); stehen vor {@code --}.
     */
    public List<String> godotArgs() {
        List<String> args = new ArrayList<>();
        if (width > 0 && height > 0) {
            args.add("--position");
            args.add(x + "," + y);
            args.add("--resolution");
            args.add(width + "x" + height);
        }
        if (mode == Mode.MAXIMIZED) {
            args.add("--maximized");
        } else if (mode == Mode.FULLSCREEN) {
            args.add("--fullscreen");
        }
        return args;
    }

    /** Aus dem Befehl {@code window} des Tisches; {@code null}, wenn er unvollständig ist. */
    static TableWindow fromJson(JsonObject cmd) {
        try {
            String m = cmd.has("mode") ? cmd.get("mode").getAsString() : "windowed";
            Mode mode = switch (m) {
                case "maximized" -> Mode.MAXIMIZED;
                case "fullscreen" -> Mode.FULLSCREEN;
                default -> Mode.WINDOWED;
            };
            return new TableWindow(cmd.get("x").getAsInt(), cmd.get("y").getAsInt(),
                    cmd.get("width").getAsInt(), cmd.get("height").getAsInt(), mode);
        } catch (RuntimeException e) {
            return null;
        }
    }
}
