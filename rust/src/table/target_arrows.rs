//! TargetArrows: leuchtende Pfeile auf dem Spieltisch (Zielwahl, Kampf, Ziele auf dem Stack).
//!
//! Nachbau des Pfeils aus `Assets/Pfeil/MTG_Pfeil.blend` (Geometry-Nodes-Gruppe „MTG_Pfeil_GN“,
//! Material „MTG_Pfeil_Mat“):
//! - **Form:** in der Tischebene gerade von Start `S` nach Ziel `Z` (anders als in Blender ohne
//!   seitlichen Bogen; der Pfeil biegt und schwingt auch nicht, wenn sich ein Ende bewegt – er
//!   folgt der Maus ruhig). Liegen Start und Ziel übereinander (näher als
//!   [`LOOP_DISTANCE`], z. B. Zauber auf dem Stack → Zauber darunter), wird er zum **Bogen**, der
//!   aus der Quelle aufsteigt und auf dem Ziel landet. Die Enden liegen in der Höhe ihrer Karten.
//!   Darüber ein flaches
//!   Band (Profil von −2·[`BREITE`] bis +2·[`BREITE`], Rest ist Leuchten). Auf den letzten
//!   [`SPITZE_LAENGE`] (höchstens halbe Länge) springt die Breite auf das [`SPITZE_BREITE`]-fache
//!   und läuft bis zur Spitze linear auf 0 – die Pfeilspitze.
//! - **Erscheinen:** `Fortschritt` 0 → 1 in Frame 1 … 9 bei 24 fps (≈ 0,33 s, weich): der Pfeil
//!   wächst vom Start zum Ziel (Teilkurve nach de Casteljau). Danach steht er.
//! - **Material:** `q = |quer − 0,5|·2` (0 = Mitte, 1 = Rand); Kern
//!   `1 − smoothstep(0,2; 0,262; q)`, heller Mittelstreifen `1 − smoothstep(0; 0,075; q)`,
//!   Leuchten `(1 − q)^2,5 · 0,55`; Deckkraft `max(Kern, Leuchten) · smoothstep(0; 0,5; Länge)`.
//!   Farbe = Mischung aus Grund- und Hellfarbe der Farbpalette ([`ArrowColor`]); Stärke aus
//!   fBm-Rauschen (Skala 3, Detail 3) an `(Länge − 1,5·t, quer)` → 0,975 … 1,625: Lichtwellen
//!   laufen zur Spitze. Werte über 1 leuchten über das Glow der Umgebung (in Blender: Bloom).
//!
//! Alle Pfeile liegen in einem `ImmediateMesh`, das jedes Frame aus den übergebenen
//! [`ArrowSpec`]s neu gebaut wird (wenige Pfeile – günstig). Der Tiefentest bleibt aktiv:
//! Gezoomte Karten vor der Kamera verdecken die Pfeile.

use std::collections::HashMap;

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::mesh::PrimitiveType;
use godot::classes::{ImmediateMesh, INode3D, MeshInstance3D, ShaderMaterial};
use godot::prelude::*;

use super::shader_noise::PERLIN_FBM;

/// Blender-Eingang „Breite“ (dort 0,14; hier etwas kräftiger für die weiter entfernte
/// Tischkamera). Das Band ist 4 × so breit, der sichtbare Kern etwa so breit wie dieser Wert.
const BREITE: f32 = 0.18;
/// Länge der Pfeilspitze und ihre Breite relativ zum Schaft.
const SPITZE_LAENGE: f32 = 0.4;
const SPITZE_BREITE: f32 = 2.4;
/// Abstand der Kurvenpunkte (Blender „Aufloesung“) und Grenzen der Punktzahl.
const AUFLOESUNG: f32 = 0.02;
const MIN_POINTS: usize = 32;
const MAX_POINTS: usize = 240;
/// Dauer des Erscheinens (Blender: Frame 1 → 9 bei 24 fps).
const GROW_TIME: f32 = 8.0 / 24.0;
/// Höhe über dem Tisch (über flach liegenden Karten) bzw. über der Karte am Pfeilende (Pfeile
/// liegen in der Höhe ihrer Karten, z. B. auf dem Stapel des Stacks).
const END_LIFT: f32 = 0.06;
const CARD_LIFT: f32 = 0.03;
/// Liegen Start und Ziel näher als das (waagerecht), wird der Pfeil zum **Bogen**: er steigt aus
/// der Quelle auf und landet auf dem Ziel (z. B. Zauber auf dem Stack → Zauber darunter).
pub const LOOP_DISTANCE: f32 = 1.0;
/// Bogen: Richtung (Tisch, Bildschirm oben links – dorthin liegen ältere Stack-Einträge), Höhe
/// des Scheitels, Start in der oberen Karte bzw. Landepunkt auf dem sichtbaren Rand der Karte
/// darunter (Abstand vom Kartenmittelpunkt in Bogenrichtung), seitlicher Abstand der Enden und
/// Anhebung des Scheitels.
const LOOP_DIR: Vector3 = Vector3::new(-0.41, 0.0, -0.91);
const LOOP_HEIGHT: f32 = 1.6;
const LOOP_START: f32 = 0.8;
const LOOP_LAND: f32 = 1.25;
const LOOP_SPREAD: f32 = 0.3;
const LOOP_RISE: f32 = 0.25;
/// Kürzeste Pfeillänge; kürzere Pfeile werden nicht gezeichnet.
const MIN_LENGTH: f32 = 0.15;

/// Farben des Pfeils (Index in die Farbpalette des Blender-Materials „MTG_Pfeil_Mat“).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ArrowColor {
    /// Gold (Weiß) – Zielwahl.
    Gold = 0,
    /// Blau – Blocker.
    Blue = 1,
    /// Violett mit schwarzem Kern (Schwarz) – Ziele auf dem Stack.
    Black = 2,
    /// Rot – Angreifer.
    Red = 3,
    Green = 4,
    /// Grau (farblos).
    Gray = 5,
    /// Orange – geplanter Block (Blocker → Angreifer).
    Orange = 6,
    /// Gelb – erklärter Block (Video „Blocken“).
    Yellow = 7,
}

/// Pfeil-Shader (Nachbau von „MTG_Pfeil_Mat“). UV.x = Länge ab Start (Welteinheiten),
/// UV.y = quer (0 … 1), UV2.x = Farbindex, UV2.y = Deckkraft.
const SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_mix, cull_disabled, shadows_disabled, depth_draw_never;

// Farbpalette (Color Ramp / Color Ramp.001, konstant, 7 Einträge wie in Blender, linear; dazu Gelb für erklärte Blocks)
const vec3 BASE[8] = vec3[8](
    vec3(1.0, 0.86, 0.5), vec3(0.0, 0.5, 1.0), vec3(0.32, 0.0, 0.5), vec3(1.0, 0.008, 0.004),
    vec3(0.05, 0.85, 0.12), vec3(0.45, 0.47, 0.52), vec3(1.0, 0.11, 0.0), vec3(1.0, 0.6, 0.0));
const vec3 HIGH[8] = vec3[8](
    vec3(1.0, 1.0, 0.92), vec3(0.65, 0.95, 1.0), vec3(0.0, 0.0, 0.0), vec3(1.0, 0.55, 0.45),
    vec3(0.8, 1.0, 0.75), vec3(1.0, 1.0, 1.0), vec3(1.0, 0.88, 0.6), vec3(1.0, 0.97, 0.62));

%NOISE%

void fragment() {
    float laenge = UV.x;
    float quer = UV.y;
    int idx = clamp(int(UV2.x + 0.5), 0, 7);
    float q = abs(quer - 0.5) * 2.0;
    float core = 1.0 - smoothstep(0.2, 0.262, q);
    float center = 1.0 - smoothstep(0.0, 0.075, q);
    float glow = pow(clamp(1.0 - q, 0.0, 1.0), 2.5) * 0.55;
    float alpha = clamp(max(core, glow) * smoothstep(0.0, 0.5, laenge), 0.0, 1.0);
    vec3 col = mix(BASE[idx], HIGH[idx], center * core);
    float n = fbm(vec4(vec3(laenge - 1.5 * TIME, quer, 0.0) * 3.0, 0.0), 4, 0.5);
    float strength = (0.75 + clamp((n - 0.3) / 0.4, 0.0, 1.0) * 0.5) * 1.3;
    ALBEDO = col * strength;
    ALPHA = alpha * UV2.y;
}
"#;

/// Ein Pfeil: von `from` nach `to` (Weltkoordinaten, Höhe wird auf den Tisch gelegt).
/// `key` kennzeichnet den Pfeil über Frames hinweg (gleicher Schlüssel = derselbe Pfeil: Biegung
/// läuft weiter, Wachsen nur beim ersten Erscheinen), `alpha` dämpft ihn (z. B. Vorschau unter
/// der Maus), `grow` = beim Erscheinen vom Start zum Ziel wachsen (aus bei Pfeilen, die der
/// Spieler gerade selbst gezogen hat), `straight` = nie zum Bogen werden, auch wenn die Enden
/// nah beieinander liegen (Kampfpfeile zwischen gegenüberliegenden Karten).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ArrowSpec {
    pub key: u64,
    pub from: Vector3,
    pub to: Vector3,
    pub color: ArrowColor,
    pub alpha: f32,
    pub grow: bool,
    pub straight: bool,
}

/// Laufender Zustand eines Pfeils (je Schlüssel).
#[derive(Clone, Copy, Debug)]
struct ArrowState {
    born: f64,
}

/// Zeichnet die leuchtenden Pfeile aus `Assets/Pfeil/MTG_Pfeil.blend` auf dem Tisch (Angriff,
/// Block, Zielwahl, Ziele auf dem Stack). Wird vom [ForgeTable] erzeugt und jedes Frame mit den
/// aktuellen Pfeilen gefüttert; Höhe des Tisches = y dieses Nodes. Neue Pfeile wachsen in
/// 0,33 s vom Start zum Ziel.
#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct TargetArrows {
    mesh: Gd<ImmediateMesh>,
    material: Gd<ShaderMaterial>,
    /// Zustand je Pfeil-Schlüssel (Zeitpunkt des Erscheinens).
    states: HashMap<u64, ArrowState>,
    /// Laufende Spielzeit dieses Knotens (Summe der Frame-Zeiten).
    clock: f64,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for TargetArrows {
    fn init(base: Base<Node3D>) -> Self {
        let shader = crate::table::shared::shader(&SHADER.replace("%NOISE%", PERLIN_FBM));
        let mut material = ShaderMaterial::new_gd();
        material.set_shader(&shader);
        // Über Tisch und flach liegenden Karten zeichnen
        material.set_render_priority(10);
        Self { mesh: ImmediateMesh::new_gd(), material, states: HashMap::new(), clock: 0.0, base }
    }

    fn ready(&mut self) {
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_name("Arrows");
        mi.set_mesh(&self.mesh);
        mi.set_material_override(&self.material);
        mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        self.base_mut().add_child(&mi);
    }

    fn process(&mut self, delta: f64) {
        self.clock += delta;
    }
}

#[godot_api]
impl TargetArrows {
    /// Setzt Pfeile aus Skripten: `points` enthält je zwei Punkte (Start, Ziel; Welt) pro Pfeil,
    /// `colors` einen Farbindex pro Pfeil (0 Gold, 1 Blau, 2 Schwarz, 3 Rot, 4 Grün, 5 Grau,
    /// 6 Orange, 7 Gelb; fehlt er: Gold). Der [ForgeTable] überschreibt die Pfeile jedes Frame, solange
    /// er läuft.
    #[func]
    pub fn set_arrow_pairs(&mut self, points: PackedVector3Array, colors: PackedInt32Array) {
        let pts = points.as_slice();
        let specs: Vec<ArrowSpec> = pts
            .chunks_exact(2)
            .enumerate()
            .map(|(i, p)| ArrowSpec {
                key: i as u64,
                from: p[0],
                to: p[1],
                color: colors.as_slice().get(i).map_or(ArrowColor::Gold, |c| color_from_index(*c)),
                alpha: 1.0,
                grow: true,
                straight: false,
            })
            .collect();
        self.set_arrows(&specs);
    }
}

impl TargetArrows {
    /// Ersetzt alle Pfeile durch `arrows` und baut das Mesh neu.
    pub fn set_arrows(&mut self, arrows: &[ArrowSpec]) {
        self.mesh.clear_surfaces();
        let now = self.clock;
        self.states.retain(|k, _| arrows.iter().any(|a| a.key == *k));
        let table_y = self.base().get_global_position().y;
        let inv = self.base().get_global_transform().affine_inverse();
        // Zu kurze Pfeile (z. B. Fähigkeit, die noch auf ihrer Quelle liegt) entfallen; ohne
        // einen einzigen Pfeil keine Fläche anlegen (Godot meldet sonst einen Fehler).
        let mut ends = Vec::new();
        for a in arrows {
            let lift = |p: Vector3| inv * Vector3::new(p.x, (p.y + CARD_LIFT).max(table_y + END_LIFT), p.z);
            let (mut from, mut to) = (lift(a.from), lift(a.to));
            let flat = Vector3::new(to.x - from.x, 0.0, to.z - from.z).length();
            // Bogen bei übereinanderliegenden Karten, sonst gerader/dynamisch gebogener Pfeil
            let arch = (!a.straight && flat < LOOP_DISTANCE).then(|| {
                let dir = LOOP_DIR.normalized();
                let side = dir.cross(Vector3::UP).normalized();
                // beide Enden über der oberen Karte, sonst verdeckt sie den Bogen
                let top = from.y.max(to.y);
                from.y = top;
                to.y = top;
                from += dir * LOOP_START + side * LOOP_SPREAD;
                to += dir * LOOP_LAND - side * LOOP_SPREAD;
                let mut p = (from + to) * 0.5 + dir * (2.0 * LOOP_HEIGHT);
                p.y = from.y.max(to.y) + LOOP_RISE;
                p
            });
            let length = (to - from).length();
            if length < MIN_LENGTH && arch.is_none() {
                continue;
            }
            let born = if a.grow { now } else { now - GROW_TIME as f64 };
            let st = self.states.entry(a.key).or_insert(ArrowState { born });
            let k = (((now - st.born) as f32) / GROW_TIME).clamp(0.0, 1.0);
            let progress = (k * k * (3.0 - 2.0 * k)).max(0.001);
            // gerade: Kontrollpunkt in der Mitte (nur der Bogen über dem Stack ist gekrümmt)
            let control = arch.unwrap_or((from + to) * 0.5);
            ends.push((from, to, a.color, a.alpha, progress, control));
        }
        if ends.is_empty() {
            return;
        }
        self.mesh.surface_begin(PrimitiveType::TRIANGLES);
        for (from, to, color, alpha, progress, control) in ends {
            build_arrow(&mut self.mesh, from, to, color, alpha, progress, control);
        }
        self.mesh.surface_end();
    }
}

fn color_from_index(i: i32) -> ArrowColor {
    match i {
        1 => ArrowColor::Blue,
        2 => ArrowColor::Black,
        3 => ArrowColor::Red,
        4 => ArrowColor::Green,
        5 => ArrowColor::Gray,
        6 => ArrowColor::Orange,
        7 => ArrowColor::Yellow,
        _ => ArrowColor::Gold,
    }
}

/// Kontrollpunkte der (Teil-)Kurve bis `progress` (0 … 1) mit Kontrollpunkt `p` der ganzen
/// Kurve: Start, Mitte, Ende (de Casteljau).
fn control_points(from: Vector3, to: Vector3, progress: f32, p: Vector3) -> [Vector3; 3] {
    let mid = from.lerp(p, progress);
    let end = mid.lerp(p.lerp(to, progress), progress);
    [from, mid, end]
}

fn bezier(c: &[Vector3; 3], t: f32) -> Vector3 {
    let u = 1.0 - t;
    c[0] * (u * u) + c[1] * (2.0 * u * t) + c[2] * (t * t)
}

/// Ein Pfeil als Band aus Vierecken (zwei Punkte quer, wie Blenders Profil-Linie).
fn build_arrow(mesh: &mut Gd<ImmediateMesh>, from: Vector3, to: Vector3, color: ArrowColor, alpha: f32, progress: f32, control: Vector3) {
    let c = control_points(from, to, progress, control);
    let n = (((from.distance_to(control) + control.distance_to(to)) / AUFLOESUNG) as usize).clamp(MIN_POINTS, MAX_POINTS);
    let pts: Vec<Vector3> = (0..=n).map(|i| bezier(&c, i as f32 / n as f32)).collect();
    let mut dist = vec![0.0f32];
    for w in pts.windows(2) {
        dist.push(dist.last().unwrap() + (w[1] - w[0]).length());
    }
    let total = *dist.last().unwrap();
    let tip = SPITZE_LAENGE.min(total * 0.5).max(1e-4);
    let tip_start = total - tip;
    let side_at = |i: usize| {
        let tangent = pts[(i + 1).min(n)] - pts[i.saturating_sub(1)];
        let s = tangent.cross(Vector3::UP);
        if s.length_squared() < 1e-8 { Vector3::RIGHT } else { s.normalized() }
    };
    // Querschnitte: (Punkt, Seitenrichtung, Länge ab Start, Breitenfaktor); am Spitzenansatz
    // zweimal (Schaft endet mit 1, Spitze beginnt mit SPITZE_BREITE)
    let mut rows: Vec<(Vector3, Vector3, f32, f32)> = Vec::with_capacity(n + 3);
    for i in 0..=n {
        if i > 0 && dist[i - 1] < tip_start && dist[i] >= tip_start {
            let f = (tip_start - dist[i - 1]) / (dist[i] - dist[i - 1]).max(1e-6);
            let p = pts[i - 1].lerp(pts[i], f);
            rows.push((p, side_at(i), tip_start, 1.0));
            rows.push((p, side_at(i), tip_start, SPITZE_BREITE));
        }
        let rest = total - dist[i];
        let scale = if rest < tip { rest / tip * SPITZE_BREITE } else { 1.0 };
        rows.push((pts[i], side_at(i), dist[i], scale));
    }
    let uv2 = Vector2::new(color as i32 as f32, alpha);
    let mut vertex = |p: Vector3, len: f32, quer: f32| {
        mesh.surface_set_uv(Vector2::new(len, quer));
        mesh.surface_set_uv2(uv2);
        mesh.surface_add_vertex(p);
    };
    for w in rows.windows(2) {
        let (p0, s0, d0, k0) = w[0];
        let (p1, s1, d1, k1) = w[1];
        let h0 = s0 * (2.0 * BREITE * k0);
        let h1 = s1 * (2.0 * BREITE * k1);
        vertex(p0 - h0, d0, 0.0);
        vertex(p0 + h0, d0, 1.0);
        vertex(p1 + h1, d1, 1.0);
        vertex(p0 - h0, d0, 0.0);
        vertex(p1 + h1, d1, 1.0);
        vertex(p1 - h1, d1, 0.0);
    }
}
