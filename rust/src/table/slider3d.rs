//! `Slider3D`: waagerechter Schieberegler in der 3D-Szene (Baustein „Bedienobjekt“ der
//! 3D-Oberfläche), z. B. um einen langen Kartenfächer nach links und rechts zu schieben.
//!
//! Aufbau: dunkle Schiene mit blauem Lichtsaum, darauf ein orange leuchtender Griff (Farben wie
//! der Rahmen-Glow der Karten: blau = bedienbar, orange = aktiv). Kollisionsbox auf dem
//! Physik-Layer [`CONTROL_LAYER`]; der [`TableController`](super::table_controller::TableController)
//! erkennt Hover, Drücken und Ziehen per Raycast und ruft [`Slider3D::drag_to`] auf. Klick auf
//! die Schiene springt dorthin. Wert 0 = ganz links, 1 = ganz rechts.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::{Area3D, BoxShape3D, CollisionShape3D, IArea3D, MeshInstance3D, QuadMesh, ShaderMaterial};
use godot::prelude::*;

use super::layers::CONTROL_LAYER;

/// Höhe der Schiene und Maße des Griffs (Welteinheiten).
const TRACK_HEIGHT: f32 = 0.05;
const KNOB_SIZE: Vector2 = Vector2::new(0.42, 0.13);
/// Trefferhöhe (großzügiger als die sichtbare Schiene).
const HIT_HEIGHT: f32 = 0.2;

/// Schiene: abgerundeter dunkler Balken mit blauem Saum.
const TRACK_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_mix, cull_disabled, depth_draw_never, shadows_disabled;
uniform vec2 size = vec2(3.0, 0.05);
uniform float hover = 0.0;
void fragment() {
    vec2 p = (UV - 0.5) * (size + vec2(0.12));
    vec2 q = abs(p) - size * 0.5 + vec2(size.y * 0.5);
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - size.y * 0.5;
    float fill = 1.0 - smoothstep(-0.004, 0.004, d);
    float rim = exp(-abs(d) / 0.012) * (1.0 - smoothstep(0.02, 0.055, d));
    vec3 col = mix(vec3(0.03, 0.04, 0.08), vec3(0.2, 0.5, 1.0), rim * (0.6 + 0.4 * hover));
    ALBEDO = col;
    ALPHA = clamp(fill * 0.85 + rim * 0.6, 0.0, 1.0);
}
"#;

/// Griff: Pille mit orangem Kern und Schein.
const KNOB_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_mix, cull_disabled, depth_draw_never, shadows_disabled;
uniform vec2 size = vec2(0.42, 0.13);
uniform float active = 0.0;
void fragment() {
    vec2 p = (UV - 0.5) * (size + vec2(0.16));
    vec2 q = abs(p) - size * 0.5 + vec2(size.y * 0.5);
    float d = length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - size.y * 0.5;
    float fill = 1.0 - smoothstep(-0.004, 0.004, d);
    float glow = exp(-max(d, 0.0) / (0.03 + 0.02 * active)) * (1.0 - smoothstep(0.03, 0.075, d));
    float t = clamp(0.5 - p.y / size.y, 0.0, 1.0);
    vec3 body = mix(vec3(1.0, 0.55, 0.15), vec3(0.85, 0.2, 0.02), t);
    vec3 col = mix(vec3(1.0, 0.3, 0.02), body, fill);
    ALBEDO = col * (1.0 + 0.4 * active);
    ALPHA = clamp(fill + glow * 0.7 * (1.0 - fill), 0.0, 1.0);
}
"#;

/// Waagerechter Schieberegler in der 3D-Szene: Schiene der Breite [member width] mit orangem
/// Griff. [member value] 0 … 1 (links … rechts); jede Änderung meldet [signal value_changed].
/// Bedient wird er per Maus über den [TableController] (Ziehen am Griff oder Klick auf die
/// Schiene).
#[derive(GodotClass)]
#[class(base=Area3D)]
pub struct Slider3D {
    /// Länge der Schiene (Welteinheiten).
    #[export]
    width: f32,
    /// Stellung 0 (links) … 1 (rechts).
    #[export]
    #[var(set = set_value)]
    value: f32,
    knob: Option<Gd<MeshInstance3D>>,
    knob_material: Option<Gd<ShaderMaterial>>,
    track_material: Option<Gd<ShaderMaterial>>,
    hovered: bool,
    dragging: bool,
    base: Base<Area3D>,
}

#[godot_api]
impl IArea3D for Slider3D {
    fn init(base: Base<Area3D>) -> Self {
        Self {
            width: 3.0,
            value: 0.5,
            knob: None,
            knob_material: None,
            track_material: None,
            hovered: false,
            dragging: false,
            base,
        }
    }

    fn ready(&mut self) {
        self.build();
    }
}

#[godot_api]
impl Slider3D {
    /// Die Stellung hat sich geändert (0 … 1).
    #[signal]
    fn value_changed(value: f32);

    /// Stellung setzen (0 … 1, wird begrenzt) und melden.
    #[func]
    pub fn set_value(&mut self, value: f32) {
        let v = value.clamp(0.0, 1.0);
        if (v - self.value).abs() < 1e-5 {
            return;
        }
        self.value = v;
        self.place_knob();
        self.signals().value_changed().emit(v);
    }

    /// Maus über dem Regler (vom [TableController]): Saum und Griff heller.
    #[func]
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        self.refresh_look();
    }

    /// Wird gerade gezogen (vom [TableController]): Griff leuchtet stärker.
    #[func]
    pub fn set_dragging(&mut self, dragging: bool) {
        self.dragging = dragging;
        self.refresh_look();
    }

    /// Griff zu dem Weltpunkt `point` schieben (Lage entlang der Schiene).
    #[func]
    pub fn drag_to(&mut self, point: Vector3) {
        let local = self.base().get_global_transform().affine_inverse() * point;
        let v = local.x / self.width.max(0.01) + 0.5;
        self.set_value(v);
    }
}

impl Slider3D {
    /// Neuer Regler mit Schienenlänge `width` und Stellung `value`.
    pub fn create(width: f32, value: f32) -> Gd<Slider3D> {
        let mut s = Slider3D::new_alloc();
        {
            let mut m = s.bind_mut();
            m.width = width;
            m.value = value.clamp(0.0, 1.0);
        }
        s
    }

    fn build(&mut self) {
        self.base_mut().set_collision_layer(CONTROL_LAYER);
        self.base_mut().set_collision_mask(0);
        self.base_mut().set_monitoring(false);
        self.base_mut().set_monitorable(false);
        let mut shape = BoxShape3D::new_gd();
        shape.set_size(Vector3::new(self.width + KNOB_SIZE.x, HIT_HEIGHT, 0.05));
        let mut collision = CollisionShape3D::new_alloc();
        collision.set_shape(&shape);
        self.base_mut().add_child(&collision);

        let (track, track_mat) = quad(TRACK_SHADER, Vector2::new(self.width, TRACK_HEIGHT), Vector2::splat(0.12));
        self.base_mut().add_child(&track);
        let (mut knob, knob_mat) = quad(KNOB_SHADER, KNOB_SIZE, Vector2::splat(0.16));
        knob.set_position(Vector3::new(0.0, 0.0, 0.01));
        self.base_mut().add_child(&knob);
        self.knob = Some(knob);
        self.knob_material = Some(knob_mat);
        self.track_material = Some(track_mat);
        self.place_knob();
        self.refresh_look();
    }

    fn place_knob(&mut self) {
        let x = (self.value - 0.5) * self.width;
        if let Some(k) = self.knob.as_mut() {
            k.set_position(Vector3::new(x, 0.0, 0.01));
        }
    }

    fn refresh_look(&mut self) {
        let active = if self.dragging { 1.0f32 } else if self.hovered { 0.5 } else { 0.0 };
        if let Some(m) = self.knob_material.as_mut() {
            m.set_shader_parameter("active", &active.to_variant());
        }
        if let Some(m) = self.track_material.as_mut() {
            m.set_shader_parameter("hover", &(if self.hovered || self.dragging { 1.0f32 } else { 0.0 }).to_variant());
        }
    }
}

/// Fläche mit Shader `code` in Größe `size` (+ Rand `margin` für den Schein), Vorderseite +Z.
fn quad(code: &str, size: Vector2, margin: Vector2) -> (Gd<MeshInstance3D>, Gd<ShaderMaterial>) {
    let shader = crate::table::shared::shader(code);
    let mut mat = ShaderMaterial::new_gd();
    mat.set_shader(&shader);
    mat.set_shader_parameter("size", &size.to_variant());
    mat.set_render_priority(2);
    let mut mesh = QuadMesh::new_gd();
    mesh.set_size(size + margin);
    mesh.set_material(&mat);
    let mut node = MeshInstance3D::new_alloc();
    node.set_mesh(&mesh);
    node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    (node, mat)
}
