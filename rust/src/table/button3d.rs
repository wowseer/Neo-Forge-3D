//! `Button3D`: anklickbarer Knopf in der 3D-Szene (Baustein „Bedienobjekt“ der 3D-Oberfläche).
//!
//! Aufbau: eine Fläche mit dem Knopf-Shader ([`button_shader`]: Nachbau der Blender-Materialien
//! aus `Assets/Buttons`, Feuer bzw. Wolken bewegen sich wie in Blender, das Leuchten pulsiert),
//! darüber die Beschriftung als `Label3D` und eine Kollisionsbox auf dem Physik-Layer
//! [`CONTROL_LAYER`]. Den Klick erkennt der
//! [`TableController`](super::table_controller::TableController) per Raycast und ruft
//! [`Button3D::activate`] auf.
//!
//! Verhalten (Zeiten nach `docs/claude/ANIMATION.md`): Erscheinen 0,20 s Cubic-Out (wächst aus der
//! Mitte), Hover weich in ca. 0,12 s (Knopf etwas größer, Leuchten heller), Klick drückt den
//! Knopf kurz ein, Verschwinden 0,15 s. Die Oberfläche läuft in einer 8-s-Schleife (Treiber der
//! Blender-Datei), jeder Knopf mit eigenem Zeitversatz.

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::label_3d::DrawFlags;
use godot::classes::{
    Area3D, BoxShape3D, CollisionShape3D, IArea3D, Label3D, MeshInstance3D, QuadMesh, ShaderMaterial,
};
use godot::prelude::*;

use super::cost_symbols;

use super::button_shader::button_shader;
use super::layers::CONTROL_LAYER;

/// Farbstil eines Knopfes (Farben aus den Blender-Dateien): 0 = Orange (Feuer, „Button_Ornage“),
/// 1 = Blau (Wolken, „Button_Blau“), 2 = Grau (neutrale Alternative, z. B. „Normal“ statt Kicker;
/// gleiche Oberfläche wie Blau, entsättigt).
#[derive(GodotConvert, Var, Export, Clone, Copy, PartialEq, Eq, Debug, Default)]
#[godot(via = i64)]
pub enum ButtonStyle {
    #[default]
    Orange,
    Blue,
    Grey,
}

/// Shader-Farben eines Stils (linear, aus den Blender-Materialien).
struct Palette {
    ramp: [Color; 4],
    ramp_pos: [f32; 4],
    streak: Color,
    shade: Color,
    rim: Color,
    glow_inner: Color,
    glow_outer: Color,
}

fn palette(style: ButtonStyle) -> Palette {
    let c = Color::from_rgb;
    match style {
        ButtonStyle::Orange => Palette {
            ramp: [c(1.0, 0.402, 0.0802), c(0.956, 0.1559, 0.0168), c(0.8714, 0.0908, 0.008), c(0.956, 0.1714, 0.0262)],
            ramp_pos: [0.0, 0.18, 0.7, 1.0],
            streak: c(1.0, 0.4287, 0.1144),
            shade: c(0.6795, 0.0382, 0.0048),
            rim: c(1.0, 0.855, 0.4287),
            glow_inner: c(0.305, 0.013, 0.1878),
            glow_outer: c(0.8308, 0.0382, 0.0908),
        },
        ButtonStyle::Blue => Palette {
            ramp: [c(0.3763, 0.6105, 1.0), c(0.1714, 0.2623, 1.0), c(0.0802, 0.1144, 1.0), c(0.1559, 0.2051, 0.956)],
            ramp_pos: [0.0, 0.2, 0.7, 1.0],
            streak: c(0.6795, 0.7913, 1.0),
            shade: c(0.0612, 0.0802, 0.8714),
            rim: c(0.7529, 0.9131, 1.0),
            glow_inner: c(0.0612, 0.013, 0.5776),
            glow_outer: c(0.1559, 0.0612, 1.0),
        },
        ButtonStyle::Grey => Palette {
            ramp: [c(0.62, 0.63, 0.66), c(0.36, 0.37, 0.4), c(0.22, 0.23, 0.25), c(0.33, 0.34, 0.37)],
            ramp_pos: [0.0, 0.2, 0.7, 1.0],
            streak: c(0.8, 0.81, 0.84),
            shade: c(0.12, 0.12, 0.14),
            rim: c(0.9, 0.91, 0.93),
            glow_inner: c(0.1, 0.1, 0.12),
            glow_outer: c(0.45, 0.46, 0.5),
        },
    }
}

/// Form (Blender-Einheiten, Bildausschnitt 5,3 × 1,4832): Höhe der Bildfläche, Eckenradius des
/// abgerundeten Rechtecks und Rand für das Leuchten links/rechts.
const CANVAS_HEIGHT: f32 = 1.4832;
const CORNER: f32 = 0.405;
const GLOW_MARGIN: f32 = 0.45;
/// Halbe Länge des geraden Knopfkörpers: breite Pille wie in Blender bzw. kurz (nur für ein,
/// zwei Symbole).
pub const WIDE_BODY: f32 = 1.795;
pub const COMPACT_BODY: f32 = 0.12;
/// Anteil der Knopf-Fläche an der Bildhöhe (Rest ist Leuchten).
const FACE_HEIGHT_FRACTION: f32 = 0.634;

/// Breite der Bildfläche (Blender-Einheiten) bei halber Körperlänge `body`.
fn canvas_width(body: f32) -> f32 {
    2.0 * (body + CORNER + GLOW_MARGIN)
}

/// Seitenverhältnis der Bildfläche.
fn aspect(body: f32) -> f32 {
    canvas_width(body) / CANVAS_HEIGHT
}

/// Anteil der Knopf-Fläche an der Bildbreite (Rest ist Leuchten).
fn face_width_fraction(body: f32) -> f32 {
    (body + CORNER) / (body + CORNER + GLOW_MARGIN)
}
/// Erscheinen, Verschwinden, Hover-Glättung, Klick-Eindrücken (Sekunden).
const APPEAR_TIME: f32 = 0.20;
const DISMISS_TIME: f32 = 0.15;
const HOVER_SPEED: f32 = 25.0;
const PRESS_TIME: f32 = 0.12;
/// Abstand vor der Kamera für Knöpfe, die an einer Bildschirmstelle stehen (vor allen Karten).
const SCREEN_DEPTH: f32 = 1.2;

/// Bildschirmstelle eines angehefteten Knopfes: Mitte = `anchor` × Fenstergröße + `offset`
/// (Pixel), Breite der Bildfläche in Pixeln.
#[derive(Clone, Copy, Debug)]
struct ScreenPin {
    anchor: Vector2,
    offset: Vector2,
    width_px: f32,
}

/// Anklickbarer Knopf in der 3D-Szene: bewegte Oberfläche und Leuchten im Farbstil
/// [member style] (Nachbau der Blender-Materialien) mit Beschriftung ([member text]). Wird vom [TableController] per
/// Raycast angeklickt (Kollisions-Layer 3) und meldet das mit [signal pressed]. Richtet sich
/// auf Wunsch zur Kamera aus ([member face_camera]).
#[derive(GodotClass)]
#[class(base=Area3D)]
pub struct Button3D {
    /// Farbstil: 0 = Orange (Feuer), 1 = Blau (Wolken), 2 = Grau. Lässt sich jederzeit
    /// wechseln (z. B. „Kein Angriff“ blau → „Angriff abbrechen“ grau).
    #[export]
    #[var(set = set_style)]
    style: ButtonStyle,
    /// Beschriftung auf dem Knopf.
    #[export]
    #[var(pub)]
    text: GString,
    /// Breite der Bildfläche in Welteinheiten (Höhe folgt aus dem Seitenverhältnis).
    #[export]
    width: f32,
    /// Knopf immer parallel zur Bildebene der Kamera ausrichten.
    #[export]
    face_camera: bool,
    /// Anklickbar? (Ausgegraut und ohne Klickwirkung, wenn nicht.)
    #[export]
    #[var(pub)]
    enabled: bool,
    /// Halbe Länge des geraden Knopfkörpers in Blender-Einheiten: 1,795 = breite Pille wie in
    /// Blender, 0,12 = kurzer Knopf für ein, zwei Symbole. Höhe, Rundung und Leuchten bleiben.
    #[export]
    body_half: f32,

    material: Option<Gd<ShaderMaterial>>,
    label: Option<Gd<Label3D>>,
    /// Beschriftung mit Kostensymbolen statt „{…}“ (`cost_symbols::rich_label`), sonst None.
    rich: Option<Gd<Node3D>>,
    visual: Option<Gd<Node3D>>,
    hovered: bool,
    /// 0 = nicht gehovert, 1 = gehovert (weich nachgeführt).
    hover_amount: f32,
    /// Zuletzt an den Shader gegebenes Leuchten (Hover-Verstärkung, ausgegraut).
    shown_glow: Option<(f32, f32)>,
    /// Zeit seit dem Erscheinen bzw. seit Beginn des Verschwindens.
    appear: f32,
    dismissing: Option<f32>,
    press: Option<f32>,
    screen_pin: Option<ScreenPin>,

    base: Base<Area3D>,
}

#[godot_api]
impl IArea3D for Button3D {
    fn init(base: Base<Area3D>) -> Self {
        Self {
            style: ButtonStyle::Orange,
            text: GString::new(),
            width: 2.4,
            face_camera: true,
            enabled: true,
            body_half: WIDE_BODY,
            material: None,
            label: None,
            rich: None,
            visual: None,
            hovered: false,
            hover_amount: 0.0,
            shown_glow: None,
            appear: 0.0,
            dismissing: None,
            press: None,
            screen_pin: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.build();
    }

    fn process(&mut self, delta: f64) {
        let dt = delta as f32;
        self.appear += dt;
        if let Some(pin) = self.screen_pin {
            self.follow_screen(pin);
        } else if self.face_camera {
            if let Some(cam) = self.base().get_viewport().and_then(|v| v.get_camera_3d()) {
                let basis = cam.get_global_transform().basis;
                let mut xf = self.base().get_global_transform();
                xf.basis = basis.orthonormalized().scaled(xf.basis.get_scale());
                self.place_global(xf);
            }
        }
        let target = if self.hovered && self.enabled { 1.0 } else { 0.0 };
        self.hover_amount += (target - self.hover_amount) * (1.0 - (-HOVER_SPEED * dt).exp());
        if (target - self.hover_amount).abs() < 1e-3 {
            self.hover_amount = target;
        }

        // Größe: Erscheinen (Cubic-Out), Hover, Klick, Verschwinden
        let p = (self.appear / APPEAR_TIME).min(1.0);
        let mut scale = 1.0 - (1.0 - p).powi(3);
        scale *= 1.0 + 0.05 * self.hover_amount;
        if let Some(t) = self.press.as_mut() {
            *t += dt;
            let k = (*t / PRESS_TIME).min(1.0);
            scale *= 1.0 - 0.07 * (k * std::f32::consts::PI).sin();
            if k >= 1.0 {
                self.press = None;
            }
        }
        if let Some(t) = self.dismissing.as_mut() {
            *t += dt;
            let k = (*t / DISMISS_TIME).min(1.0);
            scale *= 1.0 - k;
            if k >= 1.0 {
                self.base_mut().queue_free();
                return;
            }
        }
        // nur Geändertes setzen (läuft jedes Frame)
        if let Some(v) = self.visual.as_mut() {
            let s = Vector3::splat(scale.max(0.001));
            if v.get_scale() != s {
                v.set_scale(s);
            }
        }
        // Leuchten beim Hover heller; ausgegraut, wenn nicht anklickbar (Puls macht der Shader)
        let glow = (1.0 + 0.45 * self.hover_amount, if self.enabled { 0.0f32 } else { 1.0 });
        if self.shown_glow != Some(glow) {
            self.shown_glow = Some(glow);
            if let Some(m) = self.material.as_mut() {
                m.set_shader_parameter("glow_boost", &glow.0.to_variant());
                m.set_shader_parameter("disabled", &glow.1.to_variant());
            }
        }
    }
}

#[godot_api]
impl Button3D {
    /// Der Knopf wurde angeklickt.
    #[signal]
    fn pressed();

    /// Maus über dem Knopf (vom [TableController] gesetzt): Knopf wächst, Leuchten wird heller.
    #[func]
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
    }

    /// Ist die Maus über dem Knopf?
    #[func]
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    /// Klick auslösen (vom [TableController] bzw. von Skripten): kurzes Eindrücken und
    /// [signal pressed]. Ohne Wirkung, wenn der Knopf nicht [member enabled] ist oder gerade
    /// verschwindet.
    #[func]
    pub fn activate(&mut self) {
        if !self.enabled || self.dismissing.is_some() {
            return;
        }
        self.press = Some(0.0);
        self.signals().pressed().emit();
    }

    /// Knopf ausblenden (0,15 s) und danach freigeben; nicht mehr anklickbar.
    #[func]
    pub fn dismiss(&mut self) {
        if self.dismissing.is_none() {
            self.dismissing = Some(0.0);
            self.base_mut().set_collision_layer(0);
        }
    }

    /// Knopf an eine Bildschirmstelle heften (wie ein HUD-Element, aber in der 3D-Szene): Mitte
    /// = `anchor` × Fenstergröße + `offset` (Pixel, z. B. `anchor` (1, 1) = unten rechts),
    /// `width_px` = Breite der Bildfläche in Pixeln. Er steht dann jedes Frame kurz vor der
    /// Kamera, parallel zur Bildebene, vor allen Karten.
    #[func]
    pub fn pin_to_screen(&mut self, anchor: Vector2, offset: Vector2, width_px: f32) {
        self.screen_pin = Some(ScreenPin { anchor, offset, width_px });
    }

    /// Breite der Bildfläche in Pixeln, die zu einer Knopf-Fläche (ohne Leuchten) von `face_px`
    /// Pixeln gehört (Körperlänge `body`, z. B. [`WIDE_BODY`]).
    pub fn image_width_for_face(face_px: f32, body: f32) -> f32 {
        face_px / face_width_fraction(body)
    }

    /// Höhe der Knopf-Fläche (ohne Leuchten) in Pixeln bei `face_px` Pixeln Breite.
    pub fn face_height_for_face(face_px: f32, body: f32) -> f32 {
        Self::image_width_for_face(face_px, body) / aspect(body) * FACE_HEIGHT_FRACTION
    }

    /// Breite der Knopf-Fläche in Pixeln bei `face_height_px` Pixeln Höhe.
    pub fn face_width_for_height(face_height_px: f32, body: f32) -> f32 {
        face_height_px / FACE_HEIGHT_FRACTION * aspect(body) * face_width_fraction(body)
    }

    /// Setter für [member style]: Farben des Knopfes sofort umstellen (Oberfläche und Leuchten
    /// laufen ohne Neustart weiter).
    #[func]
    pub fn set_style(&mut self, style: ButtonStyle) {
        if self.style == style {
            return;
        }
        self.style = style;
        if let Some(m) = self.material.as_mut() {
            apply_palette(m, style);
        }
    }

    /// Beschriftung ändern (ohne Wirkung, wenn sie gleich bleibt).
    #[func]
    pub fn set_label_text(&mut self, text: GString) {
        if self.text == text && self.label.is_some() {
            return;
        }
        self.text = text.clone();
        if let Some(l) = self.label.as_mut() {
            l.set_text(&text);
        }
        self.fit_label();
    }
}

impl Button3D {
    /// Neuer Knopf im Farbstil `style` mit Beschriftung (noch nicht in der Szene).
    pub fn create(style: ButtonStyle, text: &str, width: f32) -> Gd<Button3D> {
        Self::create_shaped(style, text, width, WIDE_BODY)
    }

    /// Wie [`Self::create`], mit halber Körperlänge `body` ([`WIDE_BODY`], [`COMPACT_BODY`] …).
    pub fn create_shaped(style: ButtonStyle, text: &str, width: f32, body: f32) -> Gd<Button3D> {
        let mut b = Button3D::new_alloc();
        {
            let mut m = b.bind_mut();
            m.style = style;
            m.text = text.into();
            m.width = width;
            m.body_half = body;
        }
        b
    }

    /// Angeheftet: vor die Kamera an die Bildschirmstelle setzen, Größe in Pixeln halten.
    fn follow_screen(&mut self, pin: ScreenPin) {
        let Some(viewport) = self.base().get_viewport() else { return };
        let Some(cam) = viewport.get_camera_3d() else { return };
        let size = viewport.get_visible_rect().size;
        let point = pin.anchor * size + pin.offset;
        let origin = cam.project_position(point, SCREEN_DEPTH);
        // Welteinheiten je Pixel in dieser Tiefe (senkrechter Blickwinkel)
        let per_px = 2.0 * SCREEN_DEPTH * (cam.get_fov().to_radians() * 0.5).tan() / size.y.max(1.0);
        let scale = pin.width_px * per_px / self.width.max(0.001);
        let basis = cam.get_global_transform().basis.orthonormalized().scaled(Vector3::splat(scale));
        self.place_global(Transform3D::new(basis, origin));
    }

    /// Setzt die globale Lage, wenn sie sich (sichtbar) ändert – sonst nichts (läuft jedes Frame;
    /// jedes Setzen erneuert alle Teile des Knopfs und seine Trefferfläche).
    fn place_global(&mut self, xf: Transform3D) {
        let cur = self.base().get_global_transform();
        let near = |a: Vector3, b: Vector3| (a - b).length() < 1e-5;
        let same = near(cur.origin, xf.origin)
            && near(cur.basis.col_a(), xf.basis.col_a())
            && near(cur.basis.col_b(), xf.basis.col_b())
            && near(cur.basis.col_c(), xf.basis.col_c());
        if !same {
            self.base_mut().set_global_transform(xf);
        }
    }

    fn build(&mut self) {
        let height = self.width / aspect(self.body_half);
        self.base_mut().set_collision_layer(CONTROL_LAYER);
        self.base_mut().set_collision_mask(0);
        self.base_mut().set_monitoring(false);
        self.base_mut().set_monitorable(false);

        // Trefferfläche: nur der Knopf, nicht das Leuchten
        let mut shape = BoxShape3D::new_gd();
        shape.set_size(Vector3::new(
            self.width * face_width_fraction(self.body_half),
            height * FACE_HEIGHT_FRACTION,
            0.05,
        ));
        let mut collision = CollisionShape3D::new_alloc();
        collision.set_shape(&shape);
        self.base_mut().add_child(&collision);

        let mut visual = Node3D::new_alloc();
        visual.set_name("Visual");
        let (surface, material) = surface(self.style, Vector2::new(self.width, height), self.body_half);
        visual.add_child(&surface);

        let mut label = Label3D::new_alloc();
        label.set_name("Label");
        label.set_text(&self.text);
        label.set_font_size(96);
        label.set_outline_size(18);
        label.set_modulate(Color::from_rgb(1.0, 0.98, 0.94));
        label.set_outline_modulate(Color::from_rgba(0.12, 0.05, 0.02, 0.85));
        label.set_draw_flag(DrawFlags::SHADED, false);
        label.set_draw_flag(DrawFlags::DOUBLE_SIDED, false);
        label.set_render_priority(2);
        label.set_outline_render_priority(1);
        label.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        label.set_position(Vector3::new(0.0, 0.0, 0.01));
        visual.add_child(&label);

        self.base_mut().add_child(&visual);
        self.material = Some(material);
        self.shown_glow = None;
        self.label = Some(label);
        self.visual = Some(visual);
        self.fit_label();
    }

    /// Schriftgröße so wählen, dass der Text auf den Knopf passt (Höhe ca. 30 % des Knopfes).
    /// Kosten im Text („Bezahlen {1}{U}“) erscheinen als Mana-Symbole (`cost_symbols`).
    fn fit_label(&mut self) {
        let height = self.width / aspect(self.body_half);
        let text_height = height * FACE_HEIGHT_FRACTION * 0.42;
        let max_width = self.width * face_width_fraction(self.body_half) * 0.86;
        let text = self.text.to_string();
        let size = if cost_symbols::has_cost(&text) {
            // Breite wächst linear mit der Pixelgröße: Breite bei Pixelgröße 1 ausrechnen
            let unit = cost_symbols::rich_width(&text, 96, 1.0).max(0.001);
            text_height.min(max_width / unit * 96.0)
        } else {
            let chars = text.chars().count().max(1) as f32;
            // Breite eines Zeichens ca. 0,55 Schriftgrößen
            text_height.min(max_width / (chars * 0.55))
        };
        if let Some(l) = self.label.as_mut() {
            l.set_pixel_size(size / 96.0);
        }
        self.refresh_rich();
    }

    /// Beschriftung mit Kostensymbolen neu aufbauen (bzw. entfernen, wenn der Text keine hat).
    fn refresh_rich(&mut self) {
        if let Some(mut old) = self.rich.take() {
            old.queue_free();
        }
        let Some(mut label) = self.label.clone() else { return };
        let text = self.text.to_string();
        if !cost_symbols::has_cost(&text) {
            label.set_visible(true);
            return;
        }
        label.set_visible(false);
        let mut rich = cost_symbols::rich_label(&text, &label);
        for mut child in rich.get_children().iter_shared().filter_map(|c| c.try_cast::<Label3D>().ok()) {
            child.set_visible(true);
        }
        rich.set_position(label.get_position());
        if let Some(visual) = self.visual.as_mut() {
            visual.add_child(&rich);
        }
        self.rich = Some(rich);
    }
}

/// Farben des Stils `style` in das Material des Knopf-Shaders schreiben.
fn apply_palette(mat: &mut Gd<ShaderMaterial>, style: ButtonStyle) {
    let p = palette(style);
    let v4 = |c: Color| Vector4::new(c.r, c.g, c.b, 1.0);
    for (i, name) in ["ramp0", "ramp1", "ramp2", "ramp3"].iter().enumerate() {
        mat.set_shader_parameter(*name, &v4(p.ramp[i]).to_variant());
    }
    let rp = p.ramp_pos;
    mat.set_shader_parameter("ramp_pos", &Vector4::new(rp[0], rp[1], rp[2], rp[3]).to_variant());
    for (name, c) in [
        ("streak_color", p.streak),
        ("shade_color", p.shade),
        ("rim_color", p.rim),
        ("glow_inner", p.glow_inner),
        ("glow_outer", p.glow_outer),
    ] {
        mat.set_shader_parameter(name, &v4(c).to_variant());
    }
}

/// Fläche mit dem Knopf-Shader im Farbstil `style` (Vorderseite +Z). Jeder Knopf bekommt einen
/// zufälligen Zeitversatz, damit nebeneinanderliegende Knöpfe nicht im Gleichtakt laufen.
fn surface(style: ButtonStyle, size: Vector2, body: f32) -> (Gd<MeshInstance3D>, Gd<ShaderMaterial>) {
    let shader = crate::table::shared::shader(&button_shader());
    let mut mat = ShaderMaterial::new_gd();
    mat.set_shader(&shader);
    mat.set_render_priority(1);
    apply_palette(&mut mat, style);
    mat.set_shader_parameter("canvas", &Vector2::new(canvas_width(body), CANVAS_HEIGHT).to_variant());
    mat.set_shader_parameter("body_half", &body.to_variant());
    let offset = godot::global::randf_range(0.0, 4.0) as f32;
    mat.set_shader_parameter("time_offset", &offset.to_variant());
    let mut mesh = QuadMesh::new_gd();
    mesh.set_size(size);
    mesh.set_material(&mat);
    let mut node = MeshInstance3D::new_alloc();
    node.set_name("Surface");
    node.set_mesh(&mesh);
    node.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    (node, mat)
}
