//! `ManaWheel`: Farbrad in der 3D-Szene (Baustein „Radial-Menü“ der 3D-Oberfläche), wenn der
//! Spieler sich beim Mana-Erzeugen für eine Farbe entscheiden muss – nach dem Video des Users
//! („Mana Auswählen“).
//!
//! Aufbau: ein Ring aus farbigen Segmenten (Weiß oben, dann im Uhrzeigersinn Blau, Schwarz, Rot,
//! Grün; farbloses Mana grau) mit dunklen Manasymbolen (Sonne, Tropfen, Schädel, Flamme, Baum,
//! Raute), in der Mitte eine dunkle Scheibe mit dem Bild der Karte. Alles zeichnet ein Shader
//! auf einer Fläche. Das Rad steht an der Bildschirmstelle der Karte, um die es geht, kurz vor
//! der Kamera und vor allen Karten, in fester Pixelgröße; direkt darüber der Hinweis
//! („(Karte) Wähle eine Farbe.“).
//!
//! Bedienung: Der [`TableController`](super::table_controller::TableController) trifft die
//! Kollisionsfläche (Layer [`CONTROL_LAYER`]) per Raycast, meldet den Trefferpunkt
//! ([`ManaWheel::update_pointer`]) – das Segment darunter hellt auf – und löst beim Klick
//! [`ManaWheel::activate`] aus: Signal `picked(index)`.
//!
//! Zeiten nach `docs/claude/ANIMATION.md`: Aufgehen 0,20 s Cubic-Out (wächst aus der Karte),
//! Schließen 0,15 s linear, Hover exponentiell (≈ 0,12 s).

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::label_3d::DrawFlags;
use godot::classes::{Area3D, BoxShape3D, CollisionShape3D, INode3D, Label3D, MeshInstance3D, QuadMesh, ShaderMaterial, Texture2D};
use godot::prelude::*;

use super::card3d::Card3D;
use super::layers::CONTROL_LAYER;

/// Radius des Rads als Anteil der Bildhöhe und Abstand vor der Kamera (Welteinheiten).
const RADIUS_SCREEN: f32 = 0.11;
const DISTANCE: f32 = 3.0;
/// Innenradius (Anteil des Außenradius): darunter liegt die Mittelscheibe, kein Segment.
const INNER: f32 = 0.42;
/// Aufgehen / Schließen (Sekunden) und Glättung des Hover-Leuchtens (je Sekunde).
const APPEAR_TIME: f32 = 0.2;
const CLOSE_TIME: f32 = 0.15;
const HOVER_SPEED: f32 = 18.0;
/// Hinweis: Abstand seiner Mitte über dem Radrand (Anteil der Bildhöhe).
const PROMPT_GAP: f32 = 0.05;
/// Höhe des Hinweises oben am Bildschirm (Anteil der Bildhöhe), siehe `set_prompt_on_top`.
const PROMPT_TOP_Y: f32 = 0.07;
/// Höchstzahl der Segmente (Länge der Shader-Felder).
const MAX_SEGMENTS: usize = 6;

const SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_mix, cull_disabled, depth_draw_never, depth_test_disabled, shadows_disabled;
uniform int count = 5;
uniform vec4 seg_color[6];
uniform int seg_icon[6];
uniform float seg_hover[6];
uniform float seg_selected[6];
uniform sampler2D symbols : source_color, filter_linear_mipmap;
uniform vec4 seg_uv[6];
uniform bool multi = false;
uniform float inner = 0.42;
uniform float alpha = 1.0;
uniform sampler2D art : source_color, filter_linear;
uniform bool has_art = false;
uniform vec4 art_rect = vec4(0.08, 0.17, 0.84, 0.39);

float circle(vec2 p, vec2 c, float r) { return length(p - c) - r; }
float box(vec2 p, vec2 c, vec2 h) { vec2 d = abs(p - c) - h; return length(max(d, 0.0)) + min(max(d.x, d.y), 0.0); }
// Tropfen: Spitze oben bei y = tip, runder Bauch um (0, cy) mit Radius r
float drop(vec2 p, float cy, float r, float tip) {
    float d = circle(p, vec2(0.0, cy), r);
    if (p.y > cy) {
        float w = r * clamp((tip - p.y) / (tip - cy), 0.0, 1.0);
        d = min(d, abs(p.x) - w);
        d = max(d, p.y - tip);
    }
    return d;
}
// Symbol im Segment (q etwa in [-1, 1], y oben): < 0 = Symbol
float icon(int k, vec2 q) {
    float r = length(q);
    if (k == 0) { // Sonne
        float a = atan(q.y, q.x);
        float ray = abs(fract(a * 8.0 / 6.28318) - 0.5) * 2.0;
        float rays = (ray < 0.48 && r > 0.5 && r < 0.92) ? -1.0 : 1.0;
        return min(r - 0.4, rays);
    }
    if (k == 1) { return drop(q, -0.28, 0.5, 0.88); } // Tropfen
    if (k == 2) { // Schädel
        float d = min(circle(q, vec2(0.0, 0.15), 0.62), box(q, vec2(0.0, -0.5), vec2(0.34, 0.26)));
        d = max(d, -circle(q, vec2(-0.25, 0.12), 0.17));
        d = max(d, -circle(q, vec2(0.25, 0.12), 0.17));
        d = max(d, -box(q, vec2(0.0, -0.62), vec2(0.04, 0.16)));
        return d;
    }
    if (k == 3) { // Flamme: Tropfen mit Aussparung unten
        float d = drop(q, -0.3, 0.55, 0.92);
        return max(d, -drop(q - vec2(0.0, -0.32), -0.12, 0.24, 0.32));
    }
    if (k == 4) { // Baum
        float d = min(circle(q, vec2(0.0, 0.38), 0.4), min(circle(q, vec2(-0.36, 0.06), 0.34), circle(q, vec2(0.36, 0.06), 0.34)));
        return min(d, box(q, vec2(0.0, -0.5), vec2(0.1, 0.38)));
    }
    return abs(q.x) + abs(q.y) - 0.62; // Raute (farblos)
}

void fragment() {
    vec2 p = (UV - 0.5) * 2.0;
    p.y = -p.y;
    float r = length(p);
    float edge = fwidth(r) * 1.5;
    float outer = 1.0 - smoothstep(1.0 - edge, 1.0, r);
    if (outer <= 0.0) { discard; }
    vec3 col;
    if (r < inner) {
        // Mittelscheibe: Kartenbild abgedunkelt, dunkler Rand
        vec3 base = vec3(0.07, 0.07, 0.09);
        if (has_art) {
            vec2 n = p / inner;
            vec2 uv = art_rect.xy + vec2(n.x * 0.5 + 0.5, 0.5 - n.y * 0.5) * art_rect.zw;
            base = mix(base, texture(art, uv).rgb * 0.55, 0.85);
        }
        float ring = smoothstep(inner - 0.09, inner - 0.02, r);
        col = mix(base, vec3(0.04, 0.04, 0.05), ring);
    } else {
        // Segment: Winkel im Uhrzeigersinn ab oben, Segment 0 mittig oben
        float a = atan(p.x, p.y);
        if (a < 0.0) { a += 6.28318; }
        float seg = 6.28318 / float(count);
        int i = int(floor(mod(a + seg * 0.5, 6.28318) / seg));
        float h = seg_hover[i];
        float sel = seg_selected[i];
        col = mix(seg_color[i].rgb, vec3(1.0), 0.6 * h) * (1.0 + 0.25 * h);
        // Mehrfachwahl: nicht gewählte Segmente gedämpft, gewählte mit goldenem Außenring
        if (multi) {
            col = mix(col * 0.35, mix(col, vec3(1.0), 0.15) * 1.15, sel);
            float ring = smoothstep(0.82, 0.88, r) * sel;
            col = mix(col, vec3(1.0, 0.78, 0.25), ring);
        }
        // dunkle Fugen zwischen den Segmenten
        float local = mod(a + seg * 0.5, seg);
        float gap = min(local, seg - local) * r;
        col *= mix(0.25, 1.0, smoothstep(0.012, 0.03, gap));
        // Symbol in der Segmentmitte
        float mid = (inner + 1.0) * 0.5;
        float ca = float(i) * seg;
        vec2 c = mid * vec2(sin(ca), cos(ca));
        vec2 q = (p - c) / 0.2;
        vec4 suv = seg_uv[i];
        if (suv.z > 0.0) {
            // Symbol aus dem Symbolbogen (Kreis mit Motiv)
            if (length(q) < 1.0) {
                vec4 t = texture(symbols, suv.xy + vec2(q.x * 0.5 + 0.5, 0.5 - q.y * 0.5) * suv.zw);
                col = mix(col, t.rgb, t.a);
            }
        } else {
            float d = icon(seg_icon[i], q);
            float ink = 1.0 - smoothstep(-0.04, 0.04, d);
            col = mix(col, vec3(0.06, 0.06, 0.07), ink * 0.92);
        }
        // weicher Innenrand
        col *= mix(0.55, 1.0, smoothstep(inner, inner + 0.06, r));
    }
    ALBEDO = col;
    ALPHA = outer * alpha;
}
"#;

/// Ein Segment: Farbe und Symbol (0 = Sonne/Weiß, 1 = Tropfen/Blau, 2 = Schädel/Schwarz,
/// 3 = Flamme/Rot, 4 = Baum/Grün, 5 = Raute/farblos).
#[derive(Clone, Copy, Debug)]
pub struct WheelSegment {
    pub color: Color,
    pub icon: i32,
}

impl WheelSegment {
    /// Segment für einen Manabuchstaben (W, U, B, R, G, C); `None` für andere.
    pub fn for_letter(letter: char) -> Option<Self> {
        let (color, icon) = match letter.to_ascii_uppercase() {
            'W' => (Color::from_rgb(0.98, 0.95, 0.78), 0),
            'U' => (Color::from_rgb(0.55, 0.79, 0.95), 1),
            'B' => (Color::from_rgb(0.62, 0.6, 0.62), 2),
            'R' => (Color::from_rgb(0.95, 0.47, 0.43), 3),
            'G' => (Color::from_rgb(0.3, 0.68, 0.4), 4),
            'C' => (Color::from_rgb(0.78, 0.76, 0.74), 5),
            _ => return None,
        };
        Some(Self { color, icon })
    }
}

/// Farbrad vor der Kamera an einer Bildschirmstelle: Segmente (Farbe + Manasymbol), Kartenbild
/// in der Mitte, Hinweis darüber. Signal [signal picked] beim Klick auf ein Segment.
#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct ManaWheel {
    segments: Vec<WheelSegment>,
    /// Bildschirmstelle der Radmitte in Pixeln.
    anchor: Vector2,
    /// Karte, der das Rad folgt (Mitte ihres Bildschirm-Vierecks; z. B. wenn sie gerade vom
    /// Stack aufs Spielfeld gleitet).
    follow: Option<Gd<Card3D>>,
    art: Option<Gd<Texture2D>>,
    prompt_text: GString,
    /// Hinweis oben am Bildschirm wie ein Bühnen-Titel statt direkt über dem Rad.
    prompt_on_top: bool,
    hovered: i32,
    hover_amount: [f32; MAX_SEGMENTS],
    t: f32,
    closing: bool,
    material: Option<Gd<ShaderMaterial>>,
    mesh: Option<Gd<MeshInstance3D>>,
    hit: Option<Gd<Area3D>>,
    prompt: Option<Gd<Label3D>>,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for ManaWheel {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            segments: Vec::new(),
            anchor: Vector2::ZERO,
            follow: None,
            art: None,
            prompt_text: GString::new(),
            prompt_on_top: false,
            hovered: -1,
            hover_amount: [0.0; MAX_SEGMENTS],
            t: 0.0,
            closing: false,
            material: None,
            mesh: None,
            hit: None,
            prompt: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.build();
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        self.t += dt;
        let (scale, alpha) = if self.closing {
            let k = (self.t / CLOSE_TIME).min(1.0);
            if k >= 1.0 {
                self.base_mut().queue_free();
                return;
            }
            (1.0 - 0.4 * k, 1.0 - k)
        } else {
            let k = (self.t / APPEAR_TIME).min(1.0);
            (1.0 - (1.0 - k).powi(3), k.min(1.0))
        };
        for (i, h) in self.hover_amount.iter_mut().enumerate() {
            let target = if i as i32 == self.hovered { 1.0 } else { 0.0 };
            *h += (target - *h) * (1.0 - (-HOVER_SPEED * dt).exp());
        }
        if let Some(m) = self.material.as_mut() {
            m.set_shader_parameter("seg_hover", &PackedFloat32Array::from(&self.hover_amount[..]).to_variant());
            m.set_shader_parameter("alpha", &alpha.to_variant());
        }
        self.follow_camera(scale.max(0.01), alpha);
    }
}

#[godot_api]
impl ManaWheel {
    /// Hinweis oben am Bildschirm zeigen (wie ein Bühnen-Titel) statt über dem Rad – für Fragen
    /// der Engine, deren Rad in der Tischmitte zwischen den Medaillons steht.
    pub fn set_prompt_on_top(&mut self, on_top: bool) {
        self.prompt_on_top = on_top;
    }

    /// Ein Segment wurde angeklickt: `index` = Nummer des Segments (Reihenfolge wie übergeben).
    #[signal]
    fn picked(index: i64);

    /// Trefferpunkt des Mausstrahls (Welt) auf dem Rad: das Segment darunter wird hervorgehoben
    /// (außerhalb des Rings bzw. über der Mittelscheibe keins).
    #[func]
    pub fn update_pointer(&mut self, world: Vector3) {
        let Some(mesh) = self.mesh.as_ref() else { return };
        let xf = mesh.get_global_transform();
        let local = xf.affine_inverse() * world;
        // Quad 2×2: x rechts, y oben
        let p = Vector2::new(local.x, local.y);
        let r = p.length();
        let n = self.segments.len().max(1) as f32;
        self.hovered = if r < INNER || r > 1.05 || self.closing {
            -1
        } else {
            let mut a = p.x.atan2(p.y);
            if a < 0.0 {
                a += std::f32::consts::TAU;
            }
            let step = std::f32::consts::TAU / n;
            (((a + step * 0.5) % std::f32::consts::TAU) / step).floor() as i32
        };
    }

    /// Maus verlässt das Rad bzw. betritt es (ohne Trefferpunkt).
    #[func]
    pub fn set_hovered(&mut self, hovered: bool) {
        if !hovered {
            self.hovered = -1;
        }
    }

    /// Klick: liegt die Maus auf einem Segment, wird [signal picked] mit seiner Nummer gesendet.
    #[func]
    pub fn activate(&mut self) {
        if self.hovered >= 0 && !self.closing {
            let i = self.hovered as i64;
            self.signals().picked().emit(i);
        }
    }

    /// Mehrfachwahl: gewählte Segmente markieren (golden umrandet, die anderen gedämpft).
    pub fn set_selected(&mut self, selected: &[bool]) {
        let mut v = [0.0f32; MAX_SEGMENTS];
        for (i, s) in selected.iter().enumerate().take(MAX_SEGMENTS) {
            v[i] = if *s { 1.0 } else { 0.0 };
        }
        if let Some(m) = self.material.as_mut() {
            m.set_shader_parameter("multi", &true.to_variant());
            m.set_shader_parameter("seg_selected", &PackedFloat32Array::from(&v[..]).to_variant());
        }
    }

    /// Rad schließen (0,15 s ausblenden und schrumpfen), danach freigeben; nicht mehr bedienbar.
    #[func]
    pub fn close(&mut self) {
        if self.closing {
            return;
        }
        self.closing = true;
        self.t = 0.0;
        if let Some(mut hit) = self.hit.clone() {
            hit.set_collision_layer(0);
        }
    }
}

impl ManaWheel {
    /// Neues Rad (noch nicht in der Szene): Segmente in dieser Reihenfolge im Uhrzeigersinn ab
    /// oben, Radmitte an der Bildschirmstelle `anchor` (Pixel), `art` = Kartenbild für die Mitte,
    /// `prompt` = Hinweis über dem Rad.
    /// Folgt der Karte `follow` (falls gesetzt) statt fest an `anchor` zu stehen.
    pub fn create(segments: Vec<WheelSegment>, anchor: Vector2, follow: Option<Gd<Card3D>>, art: Option<Gd<Texture2D>>, prompt: &str) -> Gd<ManaWheel> {
        let mut w = ManaWheel::new_alloc();
        {
            let mut m = w.bind_mut();
            m.segments = segments.into_iter().take(MAX_SEGMENTS).collect();
            m.anchor = anchor;
            m.follow = follow;
            m.art = art;
            m.prompt_text = prompt.into();
        }
        w
    }

    fn build(&mut self) {
        let shader = crate::table::shared::shader(SHADER);
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        mat.set_render_priority(40);
        let n = self.segments.len();
        let mut colors = PackedColorArray::new();
        let mut icons = PackedInt32Array::new();
        for i in 0..MAX_SEGMENTS {
            let s = self.segments.get(i).copied().unwrap_or(WheelSegment { color: Color::GRAY, icon: 5 });
            colors.push(s.color);
            icons.push(s.icon);
        }
        // Symbole aus dem Symbolbogen (farblos aus Colorless.svg)
        let letters = ['W', 'U', 'B', 'R', 'G', 'C'];
        let mut uvs = PackedVector4Array::new();
        for i in 0..MAX_SEGMENTS {
            let icon = self.segments.get(i).map_or(5, |s| s.icon);
            let uv = letters
                .get(icon as usize)
                .and_then(|l| crate::game::mana_symbols::uv_rect(&l.to_string()))
                .unwrap_or(Vector4::ZERO);
            uvs.push(uv);
        }
        mat.set_shader_parameter("seg_uv", &uvs.to_variant());
        if let Some(sheet) = crate::game::mana_symbols::sheet_texture() {
            mat.set_shader_parameter("symbols", &sheet.to_variant());
        }
        mat.set_shader_parameter("count", &(n.max(1) as i32).to_variant());
        mat.set_shader_parameter("seg_color", &colors.to_variant());
        mat.set_shader_parameter("seg_icon", &icons.to_variant());
        mat.set_shader_parameter("inner", &INNER.to_variant());
        if let Some(art) = self.art.as_ref() {
            mat.set_shader_parameter("art", &art.to_variant());
            mat.set_shader_parameter("has_art", &true.to_variant());
        }
        let mut quad = QuadMesh::new_gd();
        quad.set_size(Vector2::new(2.0, 2.0));
        quad.set_material(&mat);
        let mut mesh = MeshInstance3D::new_alloc();
        mesh.set_mesh(&quad);
        mesh.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        self.base_mut().add_child(&mesh);

        let mut hit = Area3D::new_alloc();
        hit.set_collision_layer(CONTROL_LAYER);
        hit.set_collision_mask(0);
        hit.set_monitoring(false);
        hit.set_monitorable(false);
        let mut shape = BoxShape3D::new_gd();
        shape.set_size(Vector3::new(2.0, 2.0, 0.02));
        let mut col = CollisionShape3D::new_alloc();
        col.set_shape(&shape);
        hit.add_child(&col);
        self.base_mut().add_child(&hit);

        let mut prompt = Label3D::new_alloc();
        prompt.set_text(&self.prompt_text);
        prompt.set_font_size(64);
        prompt.set_outline_size(12);
        prompt.set_modulate(Color::from_rgb(0.95, 0.93, 0.88));
        prompt.set_outline_modulate(Color::from_rgba(0.02, 0.02, 0.03, 0.9));
        prompt.set_draw_flag(DrawFlags::SHADED, false);
        prompt.set_draw_flag(DrawFlags::DISABLE_DEPTH_TEST, true);
        prompt.set_render_priority(41);
        prompt.set_visible(!self.prompt_text.is_empty());
        self.base_mut().add_child(&prompt);

        self.material = Some(mat);
        self.mesh = Some(mesh);
        self.hit = Some(hit);
        self.prompt = Some(prompt);
        self.follow_camera(0.01, 0.0);
    }

    /// Rad an die Bildschirmstelle vor die Kamera setzen (Größe in Pixeln fest), Hinweis darüber.
    fn follow_camera(&mut self, scale: f32, alpha: f32) {
        if let Some(card) = self.follow.as_ref().filter(|c| c.is_instance_valid()) {
            let quad = card.bind().screen_quad();
            if quad.len() == 4 {
                self.anchor = quad.as_slice().iter().fold(Vector2::ZERO, |a, p| a + *p) * 0.25;
            }
        }
        let Some(viewport) = self.base().get_viewport() else { return };
        let Some(cam) = viewport.get_camera_3d() else { return };
        let screen = viewport.get_visible_rect().size;
        let view_h = 2.0 * DISTANCE * (cam.get_fov().to_radians() * 0.5).tan();
        let radius = view_h * RADIUS_SCREEN;
        // Rad ganz im Bild halten
        let margin = screen.y * RADIUS_SCREEN * 1.1;
        let at = Vector2::new(self.anchor.x.clamp(margin, (screen.x - margin).max(margin)), self.anchor.y.clamp(margin, (screen.y - margin).max(margin)));
        let basis = cam.get_global_transform().basis.orthonormalized();
        let center = cam.project_position(at, DISTANCE);
        let xf = Transform3D::new(basis.scaled(Vector3::splat(radius * scale)), center);
        if let Some(m) = self.mesh.as_mut() {
            m.set_global_transform(xf);
        }
        if let Some(h) = self.hit.as_mut() {
            h.set_global_transform(Transform3D::new(basis.scaled(Vector3::splat(radius)), center));
        }
        if let Some(l) = self.prompt.as_mut() {
            // oben wie ein Bühnen-Titel (Frage der Engine: über dem Rad lägen die Medaillons),
            // sonst über dem Rad (bzw. darunter, wenn oben kein Platz ist)
            let gap = screen.y * (RADIUS_SCREEN + PROMPT_GAP);
            let (x, y, size) = if self.prompt_on_top {
                (screen.x * 0.5, screen.y * PROMPT_TOP_Y, 0.7)
            } else if at.y - gap > screen.y * 0.06 {
                (at.x, at.y - gap, 0.45)
            } else {
                (at.x, at.y + gap, 0.45)
            };
            let top = cam.project_position(Vector2::new(x, y), DISTANCE);
            l.set_global_transform(Transform3D::new(basis, top));
            l.set_pixel_size(view_h / screen.y * size);
            let mut c = l.get_modulate();
            c.a = alpha;
            l.set_modulate(c);
        }
    }
}
