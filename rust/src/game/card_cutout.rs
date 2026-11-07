//! CardCutout: Hülle für HUD-Elemente, die dort ausgespart werden, wo eine 3D-Karte davor schwebt.
//!
//! Das HUD (CanvasLayer) wird immer über der 3D-Szene gezeichnet. Damit eine gezoomte
//! oder gezogene Karte trotzdem **über** einem Spieler-Medaillon liegt (das Medaillon also in
//! den Hintergrund rückt), hängt jede Medaillon-Zeile unter einem `CardCutout`. Dieser trägt ein
//! Canvas-Shader-Material, das seine Nachfahren (mit `use_parent_material`) erben: Jeder Pixel
//! innerhalb des Kartenvierecks wird verworfen. Die Aussparung ist das auf den Bildschirm
//! projizierte Viereck der Karte ([`Card3D::screen_quad`](crate::table::card3d::Card3D::screen_quad)).
//!
//! Solange eine Karte gehovert oder gezogen wird, wird der ganze Inhalt außerdem weich
//! durchsichtig ([`FADED_ALPHA`]), damit er nicht von der Karte ablenkt.
//!
//! Zusätzlich kann der Inhalt ausgegraut werden (z. B. das Gegner-Medaillon, solange die
//! Stack-Anzeige darüber liegt).
//!
//! (Früher per `clip_children` gelöst – das blendete die Medaillons in der Praxis komplett aus.)

use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{Control, IControl, ShaderMaterial};
use godot::prelude::*;

/// Deckkraft des Inhalts, während eine Karte gehovert oder gezogen wird.
pub const FADED_ALPHA: f32 = 0.3;
/// Geschwindigkeit des Ein-/Ausblendens (je Sekunde, exponentiell).
const FADE_SPEED: f32 = 10.0;

/// Canvas-Shader: verwirft Pixel im konvexen Viereck `quad` (Canvas-Koordinaten) und graut bei
/// `grey` = 1 aus. Die Canvas-Position wird im Vertex-Shader berechnet – unabhängig von
/// Fenster-Skalierung, da `unproject_position` dieselben Koordinaten liefert.
const SHADER: &str = r#"
shader_type canvas_item;
uniform vec2 quad[4];
uniform bool has_quad = false;
uniform float grey : hint_range(0.0, 1.0) = 0.0;
varying vec2 canvas_pos;

void vertex() {
    canvas_pos = (CANVAS_MATRIX * MODEL_MATRIX * vec4(VERTEX, 0.0, 1.0)).xy;
}

float edge(vec2 a, vec2 b, vec2 p) {
    vec2 e = b - a;
    vec2 d = p - a;
    return e.x * d.y - e.y * d.x;
}

void fragment() {
    if (has_quad) {
        float s0 = edge(quad[0], quad[1], canvas_pos);
        float s1 = edge(quad[1], quad[2], canvas_pos);
        float s2 = edge(quad[2], quad[3], canvas_pos);
        float s3 = edge(quad[3], quad[0], canvas_pos);
        bool all_pos = s0 >= 0.0 && s1 >= 0.0 && s2 >= 0.0 && s3 >= 0.0;
        bool all_neg = s0 <= 0.0 && s1 <= 0.0 && s2 <= 0.0 && s3 <= 0.0;
        if (all_pos || all_neg) {
            discard;
        }
    }
    vec4 c = COLOR;
    float l = dot(c.rgb, vec3(0.299, 0.587, 0.114));
    c.rgb = mix(c.rgb, vec3(l * 0.6), grey);
    COLOR = c;
}
"#;

/// Vollbild-Hülle für HUD-Elemente: Ihre Nachfahren (mit `use_parent_material = true`) werden
/// überall gezeichnet, außer innerhalb des konvexen Vierecks [member cutout]
/// (Bildschirmkoordinaten, 4 Punkte). So wirkt eine 3D-Karte, die vor einem Medaillon schwebt,
/// als läge sie darüber. Leeres [member cutout] = nichts ausgespart. Ist eine Aussparung gesetzt
/// (Karte gehovert/gezogen), wird der Inhalt zusätzlich weich auf 30 % Deckkraft ausgeblendet.
/// [member greyed] graut den Inhalt aus. Der [TurnHud] setzt beides jedes Frame.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct CardCutout {
    /// Konvexes Viereck (4 Punkte, Bildschirmkoordinaten), in dem die Nachfahren unsichtbar sind.
    /// Leer = keine Aussparung.
    #[var(set = set_cutout)]
    cutout: PackedVector2Array,
    /// Inhalt ausgrauen (graustufig und abgedunkelt), z. B. während die Stack-Anzeige darüber liegt.
    #[var(set = set_greyed)]
    greyed: bool,
    material: Gd<ShaderMaterial>,
    base: Base<Control>,
}

#[godot_api]
impl IControl for CardCutout {
    fn init(base: Base<Control>) -> Self {
        let shader = crate::table::shared::shader(SHADER);
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        Self { cutout: PackedVector2Array::new(), greyed: false, material, base }
    }

    fn ready(&mut self) {
        self.base_mut().set_anchors_preset(LayoutPreset::FULL_RECT);
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        let material = self.material.clone();
        self.base_mut().set_material(&material);
    }

    fn process(&mut self, delta: f64) {
        // Weich durchsichtig, solange eine Karte gehovert/gezogen wird
        let target = if self.cutout.len() == 4 { FADED_ALPHA } else { 1.0 };
        let mut m = self.base().get_modulate();
        if (m.a - target).abs() > 0.001 {
            m.a += (target - m.a) * (1.0 - (-FADE_SPEED * delta as f32).exp());
            if (m.a - target).abs() < 0.01 {
                m.a = target;
            }
            self.base_mut().set_modulate(m);
        }
    }
}

#[godot_api]
impl CardCutout {
    /// Setzt die Aussparung (4 Punkte oder leer).
    #[func]
    pub fn set_cutout(&mut self, quad: PackedVector2Array) {
        if quad == self.cutout {
            return;
        }
        self.cutout = quad;
        let has = self.cutout.len() == 4;
        self.material.set_shader_parameter("has_quad", &has.to_variant());
        if has {
            self.material.set_shader_parameter("quad", &self.cutout.to_variant());
        }
    }

    /// Schaltet das Ausgrauen ein oder aus.
    #[func]
    pub fn set_greyed(&mut self, on: bool) {
        if on != self.greyed {
            self.greyed = on;
            self.material.set_shader_parameter("grey", &(if on { 1.0f32 } else { 0.0 }).to_variant());
        }
    }
}
