//! Lichteffekte nach `Assets/Mana Glow Effekte/*.blend` (erzeugt von `mana_glow_generator.py`
//! des Users): **Mana bezahlen** und **Einschlag**.
//!
//! [`ManaFx`] spielt die Zeitleiste der Blender-Datei (30 fps, 80 Frames) in der Szene ab:
//! 1. Land blitzt auf (Frame 6), Rune-Kreis (zwei gegenläufige Ringe, Stern, Striche) und
//!    Mana-Kugel erscheinen, Funken sprühen.
//! 2. Ein Komet mit Schweif fliegt ab Frame 14 auf einem Bogen vom Land zur Karte (erst langsam,
//!    dann schnell, `t^1,7`; der Schweif hinkt 7 Frames hinterher).
//! 3. Einschlag in Frame 30: Blitz, Stern aus vier Strahlen, Schockwelle, 45 Funken, aufsteigende
//!    Glut; der Kartenrahmen der Zielkarte leuchtet auf ([`Card3D::flash`]).
//!
//! Mit [`ManaFx::impact`] nur der Einschlag (z. B. Kampfschaden, rot). Alle Teile sind additive,
//! unbeleuchtete Flächen in einer zur Kamera parallelen Ebene (wie die orthografische Kamera der
//! Blender-Datei); Größen in Blender-Einheiten × [`UNIT`].
//! Additiv heißt hier: vormultiplizierte Mischung mit Deckkraft 0 (`blend_premul_alpha`, `ALPHA = 0`),
//! damit die Flächen auch in der durchsichtigen Vordergrund-Ebene nur Licht addieren.
//! Farben: [`mana_color`] (die sieben Farben des Generators).

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{ImmediateMesh, INode3D, MeshInstance3D, QuadMesh, ShaderMaterial};
use godot::prelude::*;

use super::card3d::Card3D;

/// Blender-Einheit in Welteinheiten. Maßstabsgetreu wäre 0,252 (Karte 2,5 → 0,63); die
/// Tischkamera ist aber weit weg, deshalb doppelt so groß – so wirken die Effekte wie im Video.
pub const UNIT: f32 = 0.5;
/// Bildrate der Blender-Zeitleiste und letzter Frame.
const FPS: f32 = 30.0;
const FRAME_END: f32 = 80.0;
/// Zeitpunkte (Frames): Land blitzt, Komet startet, Einschlag; Schweif-Verzögerung.
const T_FLASH: f32 = 6.0;
const T_LAUNCH: f32 = 14.0;
const T_HIT: f32 = 30.0;
const TAIL_LAG: f32 = 7.0;
/// Gesamthelligkeit gegenüber Blender (dort `Globale_Helligkeit` 0,35 plus Bloom).
const GAIN: f32 = 0.35 * 0.75;
/// Abstand der Effekte vor den Karten (zur Kamera hin).
const LIFT: f32 = 0.35;

/// Farbe des Generators zu einem Mana-Buchstaben (W, U, B, R, G, C) bzw. „O“ = Orange.
pub fn mana_color(letter: &str) -> Vector3 {
    match letter {
        "W" => Vector3::new(1.0, 0.88, 0.5),
        "U" => Vector3::new(0.02, 0.28, 1.0),
        "B" => Vector3::new(0.38, 0.02, 0.9),
        "R" => Vector3::new(1.0, 0.04, 0.01),
        "G" => Vector3::new(0.04, 1.0, 0.08),
        "O" => Vector3::new(1.0, 0.22, 0.0),
        // Farblos: goldener Ton wie im Video
        _ => Vector3::new(1.0, 0.55, 0.06),
    }
}

/// Wert einer Blender-Kurve bei Frame `f` (Schlüssel `(frame, wert)`, dazwischen weich wie
/// Blenders Bezier-Standard, außerhalb gehalten).
fn keys(f: f32, k: &[(f32, f32)]) -> f32 {
    let Some(first) = k.first() else { return 0.0 };
    if f <= first.0 {
        return first.1;
    }
    for w in k.windows(2) {
        let (a, b) = (w[0], w[1]);
        if f <= b.0 {
            let t = ((f - a.0) / (b.0 - a.0).max(1e-4)).clamp(0.0, 1.0);
            let s = t * t * (3.0 - 2.0 * t);
            return a.1 + (b.1 - a.1) * s;
        }
    }
    k.last().map_or(0.0, |l| l.1)
}

/// Kugelförmiger Glanz (Blender: Spherical Gradient → Potenz, Kern weiß gemischt).
const BLOB_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_premul_alpha, cull_disabled, depth_test_disabled, depth_draw_never, shadows_disabled, fog_disabled;
instance uniform vec3 color = vec3(1.0);
instance uniform float strength = 0.0;
instance uniform float falloff = 2.0;
instance uniform float core = 0.6;
instance uniform float core_pow = 6.0;
uniform float gain = 0.2;
void fragment() {
    float fac = max(1.0 - length(UV * 2.0 - 1.0), 0.0);
    vec3 c = mix(color, vec3(1.0), clamp(pow(fac, core_pow) * core, 0.0, 1.0));
    ALBEDO = c * strength * gain * pow(fac, falloff);
    ALPHA = 0.0;
}
"#;

/// Ring (Schockwelle): Kurve mit Radius 1 und Dicke `width` (Bildfläche = Radius 1,25).
const RING_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_premul_alpha, cull_disabled, depth_test_disabled, depth_draw_never, shadows_disabled, fog_disabled;
instance uniform vec3 color = vec3(1.0);
instance uniform float strength = 0.0;
instance uniform float width = 0.03;
instance uniform float core = 0.3;
uniform float gain = 0.2;
void fragment() {
    float r = length(UV * 2.0 - 1.0) * 1.25;
    float d = abs(r - 1.0);
    float tube = 1.0 - smoothstep(width * 0.5, width * 1.5, d);
    float halo = exp(-d / (width * 4.0)) * 0.35;
    // Schein vor dem Rand der Bildfläche auf null, sonst zeichnen sich ihre Kanten ab
    float edge = 1.0 - smoothstep(1.1, 1.24, r);
    vec3 c = mix(color, vec3(1.0), core * tube);
    ALBEDO = c * strength * gain * (tube + halo) * edge;
    ALPHA = 0.0;
}
"#;

/// Rune-Kreis: äußerer Ring (r 1) und Striche drehen mit `rot_outer`, innerer Ring (0,78) und
/// Pentagramm mit `rot_inner` (Bildfläche = Radius 1,3).
const RUNE_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_premul_alpha, cull_disabled, depth_test_disabled, depth_draw_never, shadows_disabled, fog_disabled;
instance uniform vec3 color = vec3(1.0);
instance uniform float strength = 0.0;
instance uniform float rot_outer = 0.0;
instance uniform float rot_inner = 0.0;
uniform float gain = 0.2;
float seg(vec2 p, vec2 a, vec2 b) {
    vec2 pa = p - a, ba = b - a;
    float h = clamp(dot(pa, ba) / dot(ba, ba), 0.0, 1.0);
    return length(pa - ba * h);
}
vec2 rot(vec2 p, float a) { float c = cos(a), s = sin(a); return vec2(c * p.x - s * p.y, s * p.x + c * p.y); }
float line(float d, float w) { return 1.0 - smoothstep(w * 0.5, w * 1.6, d); }
void fragment() {
    vec2 p = (UV * 2.0 - 1.0) * 1.3;
    float r = length(p);
    float v = line(abs(r - 1.0), 0.036) + line(abs(r - 0.78), 0.024);
    // Striche (16, abwechselnd lang/kurz) am äußeren Ring
    vec2 po = rot(p, -rot_outer);
    float a = atan(po.y, po.x);
    float step_a = 6.28318 / 16.0;
    float k = floor(a / step_a + 0.5);
    float ad = abs(a - k * step_a) * r;
    float long_tick = mod(k, 2.0) < 0.5 ? 1.22 : 1.12;
    v += line(ad, 0.024) * step(1.05, r) * step(r, long_tick);
    // Pentagramm im inneren Ring
    vec2 pi = rot(p, -rot_inner);
    for (int i = 0; i < 5; i++) {
        float a0 = radians(90.0 + 144.0 * float(i));
        float a1 = radians(90.0 + 144.0 * float(i + 1));
        v += line(seg(pi, 0.78 * vec2(cos(a0), sin(a0)), 0.78 * vec2(cos(a1), sin(a1))), 0.02);
    }
    v = clamp(v, 0.0, 1.0) * (1.0 - smoothstep(1.24, 1.29, r));
    ALBEDO = mix(color, vec3(1.0), 0.15) * strength * gain * v;
    ALPHA = 0.0;
}
"#;

/// Kometenschweif: Band mit UV.x = Lage im Schweif (0 Ende … 1 Kopf), UV.y quer.
const TAIL_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_premul_alpha, cull_disabled, depth_test_disabled, depth_draw_never, shadows_disabled, fog_disabled;
instance uniform vec3 color = vec3(1.0);
instance uniform float strength = 0.0;
instance uniform float core = 0.0;
uniform float gain = 0.2;
void fragment() {
    float q = abs(UV.y - 0.5) * 2.0;
    float tube = 1.0 - smoothstep(0.55, 1.0, q);
    vec3 c = mix(color, vec3(1.0), core * (1.0 - q));
    ALBEDO = c * strength * gain * tube * smoothstep(0.0, 0.25, UV.x);
    ALPHA = 0.0;
}
"#;

fn material(code: &str) -> Gd<ShaderMaterial> {
    let shader = crate::table::shared::shader(code);
    let mut m = ShaderMaterial::new_gd();
    m.set_shader(&shader);
    m.set_shader_parameter("gain", &GAIN.to_variant());
    m.set_render_priority(20);
    m
}

/// Ein Funke bzw. Glutteilchen (Blender: einzelne Flächen mit Schlüsselbildern).
struct Spark {
    node: Gd<MeshInstance3D>,
    /// Ort relativ zum Ursprung (Blender-Einheiten), Geschwindigkeit je Frame.
    pos: Vector2,
    vel: Vector2,
    start: f32,
    life: f32,
    size: f32,
    drag: f32,
    gravity: f32,
    ember: bool,
    /// Ursprung: 0 = Land, 1 = Karte.
    at_card: bool,
}

/// Mana-Effekt (Land → Karte) bzw. nur Einschlag. Gibt sich nach Frame 80 selbst frei.
#[derive(GodotClass)]
#[class(base=Node3D, no_init)]
pub struct ManaFx {
    color: Vector3,
    /// Land bzw. Quelle (Welt); `None` = nur Einschlag.
    source: Option<Gd<Node3D>>,
    /// Ziel: Karte (leuchtet beim Einschlag auf) oder fester Punkt.
    target: Option<Gd<Node3D>>,
    /// Ersatzziel, falls bis zum Start des Kometen kein Ziel gesetzt wurde.
    fallback: Option<Gd<Node3D>>,
    target_point: Vector3,
    /// Zeit seit dem Start (Sekunden), Verzögerung vor dem Start.
    t: f32,
    delay: f32,
    /// Ab diesem Frame der Zeitleiste beginnen (Einschlag allein: kurz vor dem Treffer).
    first_frame: f32,
    flashed: bool,
    parts: Option<Parts>,
    sparks: Vec<Spark>,
    rng: u32,
    base: Base<Node3D>,
}

struct Parts {
    land_flash: Gd<MeshInstance3D>,
    orb: Gd<MeshInstance3D>,
    rune: Gd<MeshInstance3D>,
    head: Gd<MeshInstance3D>,
    tail_outer: (Gd<MeshInstance3D>, Gd<ImmediateMesh>),
    tail_core: (Gd<MeshInstance3D>, Gd<ImmediateMesh>),
    hit_flash: Gd<MeshInstance3D>,
    rays: Vec<(Gd<MeshInstance3D>, f32, f32)>,
    wave: Gd<MeshInstance3D>,
}

#[godot_api]
impl INode3D for ManaFx {
    fn ready(&mut self) {
        self.build();
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        if self.delay > 0.0 {
            self.delay -= dt;
            self.hide_all();
            return;
        }
        self.t += dt;
        let f = self.first_frame + self.t * FPS;
        if f > FRAME_END {
            self.base_mut().queue_free();
            return;
        }
        self.update(f);
    }
}

#[godot_api]
impl ManaFx {
    /// Ziel nachträglich setzen (z. B. sobald die Karte auf dem Stack liegt).
    #[func]
    pub fn set_target(&mut self, target: Option<Gd<Node3D>>) {
        self.target = target;
    }

    /// Hat der Effekt schon ein Ziel?
    #[func]
    pub fn has_target(&self) -> bool {
        self.target.as_ref().is_some_and(|t| t.is_instance_valid())
    }
}

impl ManaFx {
    /// Mana-Effekt vom Land `source` zur Karte `target` (kann später gesetzt werden) in `color`.
    /// `fallback` gilt, wenn bis zum Start des Kometen kein Ziel gesetzt wurde.
    pub fn mana(source: Gd<Node3D>, target: Option<Gd<Node3D>>, fallback: Option<Gd<Node3D>>, color: Vector3, delay: f32) -> Gd<ManaFx> {
        let mut fx = Self::make(Some(source), target, Vector3::ZERO, color, delay, 1.0);
        fx.bind_mut().fallback = fallback;
        fx
    }

    /// Nur der Einschlag bei `target` (Karte) bzw. am Punkt `point`, nach `delay` Sekunden.
    pub fn impact(target: Option<Gd<Node3D>>, point: Vector3, color: Vector3, delay: f32) -> Gd<ManaFx> {
        Self::make(None, target, point, color, delay, T_HIT - 1.0)
    }

    fn make(source: Option<Gd<Node3D>>, target: Option<Gd<Node3D>>, point: Vector3, color: Vector3, delay: f32, first_frame: f32) -> Gd<ManaFx> {
        let seed = (godot::global::randi() as u32) | 1;
        Gd::from_init_fn(|base| ManaFx {
            color,
            source,
            target,
            fallback: None,
            target_point: point,
            t: 0.0,
            delay,
            first_frame,
            flashed: false,
            parts: None,
            sparks: Vec::new(),
            rng: seed,
            base,
        })
    }

    fn rand(&mut self) -> f32 {
        // xorshift: deterministisch genug, ohne Abhängigkeiten
        self.rng ^= self.rng << 13;
        self.rng ^= self.rng >> 17;
        self.rng ^= self.rng << 5;
        (self.rng % 10_000) as f32 / 10_000.0
    }

    fn uniform(&mut self, a: f32, b: f32) -> f32 {
        a + (b - a) * self.rand()
    }

    fn quad(&mut self, mat: &Gd<ShaderMaterial>, half: f32) -> Gd<MeshInstance3D> {
        let mut mesh = QuadMesh::new_gd();
        mesh.set_size(Vector2::splat(2.0 * half));
        let mut node = MeshInstance3D::new_alloc();
        node.set_mesh(&mesh);
        node.set_material_override(mat);
        node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        node.set_instance_shader_parameter("color", &self.color.to_variant());
        self.base_mut().add_child(&node);
        node
    }

    fn build(&mut self) {
        let blob = material(BLOB_SHADER);
        let ring = material(RING_SHADER);
        let rune_mat = material(RUNE_SHADER);
        let tail_mat = material(TAIL_SHADER);
        let set = |n: &mut Gd<MeshInstance3D>, falloff: f32, core: f32, core_pow: f32| {
            n.set_instance_shader_parameter("falloff", &falloff.to_variant());
            n.set_instance_shader_parameter("core", &core.to_variant());
            n.set_instance_shader_parameter("core_pow", &core_pow.to_variant());
        };
        let mut land_flash = self.quad(&blob, 1.0);
        set(&mut land_flash, 2.2, 1.0, 3.0);
        let mut orb = self.quad(&blob, 1.0);
        set(&mut orb, 1.5, 1.0, 2.0);
        let rune = self.quad(&rune_mat, 1.3);
        let mut head = self.quad(&blob, 1.0);
        set(&mut head, 2.0, 1.0, 2.5);
        let mut tail = |core: f32| {
            let mesh = ImmediateMesh::new_gd();
            let mut node = MeshInstance3D::new_alloc();
            node.set_mesh(&mesh);
            node.set_material_override(&tail_mat);
            node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            node.set_instance_shader_parameter("color", &self.color.to_variant());
            node.set_instance_shader_parameter("core", &core.to_variant());
            self.base_mut().add_child(&node);
            (node, mesh)
        };
        let tail_outer = tail(0.0);
        let tail_core = tail(0.8);
        let mut hit_flash = self.quad(&blob, 1.0);
        set(&mut hit_flash, 2.5, 1.0, 2.0);
        let mut rays = Vec::new();
        for (angle, length) in [(0.0f32, 4.5f32), (90.0, 3.2), (35.0, 1.8), (-40.0, 1.6)] {
            let mut r = self.quad(&blob, 1.0);
            set(&mut r, 3.0, 1.0, 3.0);
            rays.push((r, angle.to_radians(), length));
        }
        let mut wave = self.quad(&ring, 1.25);
        wave.set_instance_shader_parameter("width", &(0.03f32 * 2.0).to_variant());
        wave.set_instance_shader_parameter("core", &0.3f32.to_variant());
        self.parts = Some(Parts { land_flash, orb, rune, head, tail_outer, tail_core, hit_flash, rays, wave });

        // Funken: am Land (Frame T_FLASH+2) und beim Einschlag (Frame T_HIT), Glut danach
        if self.source.is_some() {
            self.burst(&blob, false, T_FLASH + 2.0, 18, 0.18, 14.0, 0.86, 0.05);
        }
        self.burst(&blob, true, T_HIT, 45, 0.5, 22.0, 0.83, 0.09);
        for _ in 0..16 {
            let node = self.spark_node(&blob);
            let pos = Vector2::new(self.uniform(-1.4, 1.4), self.uniform(-1.8, 1.8));
            let start = T_HIT + self.uniform(2.0, 12.0).floor();
            let life = self.uniform(18.0, 34.0).floor();
            let vel = Vector2::new(self.uniform(-0.02, 0.02), self.uniform(0.015, 0.04));
            let size = self.uniform(0.025, 0.05);
            self.sparks.push(Spark { node, pos, vel, start, life, size, drag: 1.0, gravity: 0.0, ember: true, at_card: true });
        }
    }

    fn spark_node(&mut self, blob: &Gd<ShaderMaterial>) -> Gd<MeshInstance3D> {
        let mut n = self.quad(blob, 1.0);
        n.set_instance_shader_parameter("falloff", &2.0f32.to_variant());
        n.set_instance_shader_parameter("core", &0.8f32.to_variant());
        n.set_instance_shader_parameter("core_pow", &2.0f32.to_variant());
        n.set_instance_shader_parameter("strength", &25.0f32.to_variant());
        n.set_visible(false);
        n
    }

    #[allow(clippy::too_many_arguments)]
    fn burst(&mut self, blob: &Gd<ShaderMaterial>, at_card: bool, t0: f32, count: usize, speed: f32, life: f32, drag: f32, size: f32) {
        for _ in 0..count {
            let node = self.spark_node(blob);
            let ang = self.uniform(0.0, std::f32::consts::TAU);
            let vel = Vector2::new(ang.cos(), ang.sin()) * speed * self.uniform(0.35, 1.0);
            let start = t0 + self.uniform(0.0, 2.99).floor();
            let lf = (life * self.uniform(0.6, 1.2)).floor().max(2.0);
            let sz = size * self.uniform(0.6, 1.3);
            self.sparks.push(Spark { node, pos: Vector2::ZERO, vel, start, life: lf, size: sz, drag, gravity: -0.004, ember: false, at_card });
        }
    }

    fn hide_all(&mut self) {
        for c in self.base().get_children().iter_shared() {
            if let Ok(mut n) = c.try_cast::<Node3D>() {
                n.set_visible(false);
            }
        }
    }

    /// Zeitleiste bei Frame `f` anwenden.
    fn update(&mut self, f: f32) {
        let Some(cam) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) else { return };
        // Ebene parallel zum Bildschirm (wie die orthografische Blender-Kamera)
        let basis = cam.get_global_transform().basis.orthonormalized();
        self.base_mut().set_global_transform(Transform3D::new(basis, Vector3::ZERO));
        let to_local = |w: Vector3| {
            let l = basis.transposed() * w;
            Vector2::new(l.x, l.y)
        };
        let depth = |w: Vector3| (basis.transposed() * w).z + LIFT;
        let land_w = self.source.as_ref().filter(|s| s.is_instance_valid()).map(|s| s.get_global_position());
        if self.target.is_none() && f >= T_LAUNCH - 1.0 {
            self.target = self.fallback.take();
        }
        // erst prüfen, dann klonen (gelöschte Knoten dürfen nicht angefasst werden)
        let target_node = self.target.as_ref().filter(|t| t.is_instance_valid()).cloned();
        let card_w = target_node.as_ref().map_or(self.target_point, |t| t.get_global_position());
        // Größe des Einschlags folgt der Zielkarte (z. B. große Karten auf dem Stack)
        let card_scale = target_node.as_ref().map_or(1.0, |t| t.get_global_transform().basis.col_a().length().max(0.3).sqrt());
        let (k_land, k_card) = (UNIT, UNIT * card_scale);
        let land = land_w.map(to_local);
        let card = to_local(card_w);
        let z_land = land_w.map_or(0.0, depth);
        let z_card = depth(card_w);
        let color = self.color;
        let Some(parts) = self.parts.as_mut() else { return };

        let place = |n: &mut Gd<MeshInstance3D>, at: Vector2, z: f32, scale: Vector2, angle: f32, strength: f32| {
            n.set_visible(strength > 0.001 && scale.x > 1e-4 && scale.y > 1e-4);
            let b = Basis::from_axis_angle(Vector3::BACK, angle).scaled(Vector3::new(scale.x.max(1e-4), scale.y.max(1e-4), 1.0));
            n.set_transform(Transform3D::new(b, Vector3::new(at.x, at.y, z)));
            n.set_instance_shader_parameter("strength", &strength.to_variant());
        };

        // 2. Land: Blitz, Kugel, Rune-Kreis
        if let Some(land) = land {
            let s = keys(f, &[(T_FLASH, 0.2), (T_FLASH + 3.0, 2.0), (T_FLASH + 7.0, 1.6), (T_LAUNCH + 2.0, 1.2), (T_HIT + 4.0, 1.0), (T_HIT + 16.0, 0.8)]);
            let st = keys(f, &[(T_FLASH, 0.0), (T_FLASH + 3.0, 30.0), (T_FLASH + 7.0, 12.0), (T_LAUNCH + 2.0, 5.0), (T_HIT + 4.0, 3.0), (T_HIT + 16.0, 0.0)]);
            place(&mut parts.land_flash, land, z_land, Vector2::splat(s * k_land), 0.0, st);
            let s = keys(f, &[(T_FLASH + 1.0, 0.0), (T_FLASH + 5.0, 0.75), (T_LAUNCH, 0.6), (T_LAUNCH + 3.0, 0.45), (T_HIT + 6.0, 0.4), (T_HIT + 18.0, 0.2)]);
            let st = keys(f, &[(T_FLASH + 1.0, 0.0), (T_FLASH + 5.0, 14.0), (T_LAUNCH, 10.0), (T_LAUNCH + 3.0, 6.0), (T_HIT + 6.0, 4.0), (T_HIT + 18.0, 0.0)]);
            place(&mut parts.orb, land, z_land + 0.01, Vector2::splat(s * k_land), 0.0, st);
            let s = keys(f, &[(T_FLASH + 1.0, 0.3), (T_FLASH + 6.0, 1.15), (T_LAUNCH + 4.0, 1.0), (T_HIT + 6.0, 1.05), (T_HIT + 18.0, 1.3)]);
            let rot = keys(f, &[(T_FLASH + 1.0, 0.0), (T_FLASH + 6.0, 40.0), (T_LAUNCH + 4.0, 90.0), (T_HIT + 6.0, 200.0), (T_HIT + 18.0, 260.0)]).to_radians();
            let st = keys(f, &[(T_FLASH + 1.0, 0.0), (T_FLASH + 6.0, 10.0), (T_LAUNCH + 4.0, 6.0), (T_HIT + 6.0, 4.0), (T_HIT + 18.0, 0.0)]);
            place(&mut parts.rune, land, z_land - 0.01, Vector2::splat(s * k_land), 0.0, st);
            parts.rune.set_instance_shader_parameter("rot_outer", &rot.to_variant());
            parts.rune.set_instance_shader_parameter("rot_inner", &(-rot * 2.2).to_variant());

            // 3. Komet: Bogen vom Land zur Karte, erst langsam, dann schnell
            let length = (card - land).length();
            let ctrl = Vector2::new((land.x + card.x) * 0.5 - 0.16 * length, land.y.max(card.y) + 0.23 * length);
            let qb = |t: f32| land * ((1.0 - t) * (1.0 - t)) + ctrl * (2.0 * (1.0 - t) * t) + card * (t * t);
            let ease = |t: f32| t.clamp(0.0, 1.0).powf(1.7);
            let pe = ease((f - T_LAUNCH) / (T_HIT - T_LAUNCH));
            let ps = ease((f - T_LAUNCH - TAIL_LAG) / (T_HIT - T_LAUNCH)).min(pe);
            let zc = |t: f32| z_land + (z_card - z_land) * t;
            let head_s = keys(f, &[(T_LAUNCH - 1.0, 0.0), (T_LAUNCH + 1.0, 0.55), (T_HIT, 0.75), (T_HIT + 2.0, 0.0)]);
            place(&mut parts.head, qb(pe), zc(pe) + 0.02, Vector2::splat(0.5 * head_s * k_land), 0.0, if head_s > 0.0 { 8.0 } else { 0.0 });
            let outer_st = keys(f, &[(T_LAUNCH - 1.0, 0.0), (T_LAUNCH + 1.0, 6.0), (T_HIT + 1.0, 6.0), (T_HIT + 10.0, 0.0)]);
            let core_st = keys(f, &[(T_LAUNCH - 1.0, 0.0), (T_LAUNCH + 1.0, 18.0), (T_HIT + 1.0, 18.0), (T_HIT + 10.0, 0.0)]);
            for ((node, mesh), radius, st) in [
                (&mut parts.tail_outer, 0.16 * k_land, outer_st),
                (&mut parts.tail_core, 0.05 * k_land, core_st),
            ] {
                node.set_instance_shader_parameter("strength", &st.to_variant());
                build_tail(mesh, &qb, &zc, ps, pe, radius);
                node.set_visible(st > 0.001 && pe > ps);
            }
        }

        // 4. Einschlag
        let s = keys(f, &[(T_HIT - 1.0, 0.1), (T_HIT + 1.0, 2.0), (T_HIT + 4.0, 1.4), (T_HIT + 14.0, 0.8)]);
        let st = keys(f, &[(T_HIT - 1.0, 0.0), (T_HIT + 1.0, 25.0), (T_HIT + 4.0, 10.0), (T_HIT + 14.0, 0.0)]);
        place(&mut parts.hit_flash, card, z_card + 0.03, Vector2::splat(s * k_card), 0.0, st);
        let k = keys(f, &[(T_HIT - 1.0, 0.0), (T_HIT + 1.0, 1.0), (T_HIT + 5.0, 0.6), (T_HIT + 12.0, 0.0)]);
        let st = keys(f, &[(T_HIT - 1.0, 0.0), (T_HIT + 1.0, 30.0), (T_HIT + 12.0, 0.0)]);
        for (ray, angle, length) in parts.rays.iter_mut() {
            let scale = Vector2::new(0.5 * *length * k.max(0.01), 0.5 * 0.07 * k.max(0.01)) * k_card;
            place(ray, card, z_card + 0.04, scale, *angle, st);
        }
        let s = keys(f, &[(T_HIT, 0.1), (T_HIT + 1.0, 0.4), (T_HIT + 10.0, 1.9), (T_HIT + 16.0, 2.3)]);
        let st = keys(f, &[(T_HIT, 0.0), (T_HIT + 1.0, 25.0), (T_HIT + 10.0, 4.0), (T_HIT + 16.0, 0.0)]);
        place(&mut parts.wave, card, z_card + 0.035, Vector2::splat(s * k_card), 0.0, st);
        parts.wave.set_instance_shader_parameter("color", &color.to_variant());

        // 5. Funken und Glut (Frame für Frame wie in Blender, hier aus der Zeit berechnet)
        for sp in self.sparks.iter_mut() {
            let (origin, z, k_unit) = if sp.at_card { (card, z_card + 0.05, k_card) } else { (land.unwrap_or(card), z_land + 0.05, k_land) };
            let age = f - sp.start;
            if age < 0.0 || age > sp.life {
                sp.node.set_visible(false);
                continue;
            }
            if sp.ember {
                let fl = (std::f32::consts::PI * age / sp.life).sin().max(0.0);
                let q = sp.pos + sp.vel * age + Vector2::new(0.08 * (age * 0.3 + sp.size * 100.0).sin(), 0.0);
                place(&mut sp.node, origin + q * k_unit, z, Vector2::splat(sp.size * fl * k_unit), 0.0, 25.0);
                continue;
            }
            // Bahn mit Luftwiderstand und Schwerkraft (geschlossen über die Frames summiert)
            let n = age.floor() as i32;
            let mut p = Vector2::ZERO;
            let mut v = sp.vel;
            for _ in 0..n {
                p += v;
                v *= sp.drag;
                v.y += sp.gravity;
            }
            let fade = 1.0 - age / sp.life;
            let stretch = 1.0 + v.length() * 14.0;
            let angle = v.y.atan2(v.x);
            let scale = Vector2::new(sp.size * stretch * fade, sp.size * fade) * k_unit;
            place(&mut sp.node, origin + p * k_unit, z, scale, angle, 25.0);
        }

        // Kartenrahmen leuchtet beim Einschlag auf
        if f >= T_HIT + 1.0 && !self.flashed {
            self.flashed = true;
            if let Some(card) = target_node.and_then(|t| t.try_cast::<Card3D>().ok()) {
                let mut card = card;
                card.bind_mut().flash();
            }
        }
    }
}

/// Schweif als Band in der Effekt-Ebene zwischen den Bahnpunkten `ps` … `pe`; Breite nach der
/// Schweif-Form der Blender-Datei (dünn am Ende, voll am Kopf).
fn build_tail(mesh: &mut Gd<ImmediateMesh>, qb: &dyn Fn(f32) -> Vector2, zc: &dyn Fn(f32) -> f32, ps: f32, pe: f32, radius: f32) {
    mesh.clear_surfaces();
    if pe - ps < 1e-3 {
        return;
    }
    const N: usize = 40;
    let pts: Vec<(Vector2, f32)> = (0..=N).map(|i| {
        let t = ps + (pe - ps) * i as f32 / N as f32;
        (qb(t), zc(t))
    }).collect();
    mesh.surface_begin(PrimitiveType::TRIANGLES);
    let mut prev: Option<(Vector3, Vector3, f32)> = None;
    for i in 0..=N {
        let u = i as f32 / N as f32;
        let a = pts[i.saturating_sub(1)].0;
        let b = pts[(i + 1).min(N)].0;
        let dir = (b - a).normalized_or_zero();
        let side = Vector2::new(-dir.y, dir.x) * radius * u.powf(2.8).max(0.02);
        let (p, z) = pts[i];
        let l = Vector3::new(p.x - side.x, p.y - side.y, z);
        let r = Vector3::new(p.x + side.x, p.y + side.y, z);
        if let Some((pl, pr, pu)) = prev {
            for (v, uv) in [(pl, (pu, 0.0)), (pr, (pu, 1.0)), (r, (u, 1.0)), (pl, (pu, 0.0)), (r, (u, 1.0)), (l, (u, 0.0))] {
                mesh.surface_set_uv(Vector2::new(uv.0, uv.1));
                mesh.surface_add_vertex(v);
            }
        }
        prev = Some((l, r, u));
    }
    mesh.surface_end();
}
