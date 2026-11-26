//! `CardInspector`: Kartenansicht beim Hovern – Aufbau wie `Fähigkeiten.png` des Users.
//!
//! Statt dass die Karte selbst zur Bildmitte fliegt, zeigt der Inspektor eine **Kopie** der
//! Karte groß, hell und lesbar am Bildschirmrand gegenüber der Maus. Rechts daneben (bzw. links,
//! wenn die Karte am rechten Rand steht), oben bündig, eine dunkle Tafel mit runden Ecken:
//! je Fähigkeit ein weißes Symbol, darüber der Name groß und fett, darunter die Erklärung
//! kursiv (Schlüsselwörter mit Erinnerungstext der Engine), danach die Veränderungen (Marken,
//! Stärke/Widerstand, Schaden, angelegte Karten).
//!
//! Die Tafel wird mit Godot-Controls in einem eigenen `SubViewport` gesetzt (sauberer
//! Zeilenumbruch, fett/kursiv) und als Bild auf eine Fläche in der 3D-Szene gelegt. Der
//! Inspektor steht in festem Abstand vor der Kamera und folgt ihr; nicht anklickbar.
//!
//! Doppelseitige Karten (transformierbar, modal, Flip; CR 711, 712) zeigen wie in Arena beide
//! Seiten nebeneinander: die angezeigte Seite außen, die andere Seite daneben zur Bildmitte hin,
//! dahinter die Tafel. Bei Handkarten (die sich selbst groß zeigen) erscheint nur die andere Seite.

use godot::classes::base_material_3d::{ShadingMode, TextureParam, Transparency};
use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Control, FontVariation, HBoxContainer, INode3D, Label, MarginContainer, MeshInstance3D, PanelContainer, QuadMesh,
    StandardMaterial3D, StyleBoxFlat, SubViewport, SystemFont, VBoxContainer,
};
use godot::prelude::*;

use super::card3d::{Card3D, CARD_HEIGHT, CARD_WIDTH};
use super::placeholder;
use crate::game::cards::CardDef;

/// Abstand vor der Kamera und Höhe der Kartenkopie (Welteinheiten).
const DEPTH: f32 = 3.0;
const CARD_SHOWN_HEIGHT: f32 = 1.05;
/// So weit (Welteinheiten) liegt die Ansicht über der Bildmitte: ihre Unterkante endet über dem
/// Phasen-Zeitstrahl unten rechts (bzw. Friedhof/Manapool unten links).
const LIFT: f32 = 0.14;
/// Rand zum Bildschirmrand und Abstand Karte–Tafel (Welteinheiten).
const EDGE: f32 = 0.06;
const GAP: f32 = 0.03;
/// Tafel: Breite in Pixeln (Layout) und Welteinheiten je Pixel (Breite 0,95 Welteinheiten).
const PANEL_PX: i32 = 460;
const WORLD_PER_PX: f32 = 0.95 / PANEL_PX as f32;
/// Schriftgrößen (Pixel des Layouts) für Name und Erklärung, Symbolgröße.
const TITLE_PX: i32 = 30;
const TEXT_PX: i32 = 21;
const ICON_PX: f32 = 46.0;

/// Ein Eintrag der Tafel: Name (z. B. „Flying“) und Erklärung (darf leer sein).
#[derive(Clone, Debug, Default)]
pub struct InspectorEntry {
    pub title: String,
    pub text: String,
}

/// Kartenansicht beim Hovern wie `Fähigkeiten.png`: helle Kopie der Karte am Bildschirmrand,
/// daneben eine dunkle Tafel mit Fähigkeiten (Symbol, Name fett, Erklärung kursiv) und
/// Veränderungen. [method hide_card] blendet sie aus.
#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct CardInspector {
    /// true = Karte am rechten Bildschirmrand (Tafel links davon), false = links.
    right_side: bool,
    /// Zahl der gezeigten Kartenkopien (1, mit anderer Seite 2); die Tafel steht dahinter.
    shown_cards: usize,
    content: Option<Gd<Node3D>>,
    /// Tafel: Layout im SubViewport und die Fläche, die es zeigt.
    viewport: Option<Gd<SubViewport>>,
    panel: Option<Gd<PanelContainer>>,
    quad: Option<Gd<MeshInstance3D>>,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for CardInspector {
    fn init(base: Base<Node3D>) -> Self {
        Self { right_side: false, shown_cards: 1, content: None, viewport: None, panel: None, quad: None, base }
    }

    fn process(&mut self, _delta: f64) {
        if self.content.is_none() {
            return;
        }
        let Some(viewport) = self.base().get_viewport() else { return };
        let Some(cam) = viewport.get_camera_3d() else { return };
        let xf = cam.get_global_transform();
        let basis = xf.basis.orthonormalized();
        let forward = -basis.col_c();
        self.base_mut().set_global_transform(Transform3D::new(basis, xf.origin + forward * DEPTH));
        let size = viewport.get_visible_rect().size;
        let half_h = DEPTH * (cam.get_fov().to_radians() * 0.5).tan();
        let half_w = half_h * size.x / size.y.max(1.0);
        let card_w = CARD_WIDTH * CARD_SHOWN_HEIGHT / CARD_HEIGHT;
        let x = if self.right_side { half_w - EDGE - card_w * 0.5 } else { -half_w + EDGE + card_w * 0.5 };
        if let Some(c) = self.content.as_mut() {
            c.set_position(Vector3::new(x, LIFT, 0.0));
        }
        self.fit_panel(card_w);
    }
}

#[godot_api]
impl CardInspector {
    /// Ansicht ausblenden.
    #[func]
    pub fn hide_card(&mut self) {
        if let Some(mut c) = self.content.take() {
            c.queue_free();
        }
        self.viewport = None;
        self.panel = None;
        self.quad = None;
    }

    /// Wird gerade eine Karte gezeigt?
    #[func]
    pub fn is_showing(&self) -> bool {
        self.content.is_some()
    }
}

impl CardInspector {
    /// Karten `faces` zeigen (die erste außen am Bildschirmrand, weitere – die andere Seite
    /// doppelseitiger Karten – daneben zur Bildmitte hin); `right_side` = am rechten
    /// Bildschirmrand (sonst links); Tafel mit `abilities` (Fähigkeiten) und `buffs`
    /// (Veränderungen) – beide leer = keine Tafel.
    pub fn show(&mut self, faces: &[CardDef], abilities: &[InspectorEntry], buffs: &[InspectorEntry], right_side: bool) {
        self.hide_card();
        if faces.is_empty() {
            return;
        }
        self.right_side = right_side;
        self.shown_cards = faces.len();
        let mut content = Node3D::new_alloc();
        content.set_name("Content");

        // Kartenkopien: hell, stehend zur Kamera (Vorderseite der Karte +Y → zur Kamera)
        let scale = CARD_SHOWN_HEIGHT / CARD_HEIGHT;
        let card_w = CARD_WIDTH * scale;
        let dir = if right_side { -1.0 } else { 1.0 };
        let mut copies = Vec::new();
        for (i, def) in faces.iter().enumerate() {
            let mut copy = Card3D::new_alloc();
            {
                let mut c = copy.bind_mut();
                c.set_face_texture(Some(placeholder::card_face(def)));
                c.apply_def(def);
                c.set_bright(true);
            }
            copy.set_transform(Transform3D::new(
                Basis::from_axis_angle(Vector3::RIGHT, std::f32::consts::FRAC_PI_2).scaled(Vector3::splat(scale)),
                Vector3::new(dir * i as f32 * (card_w + GAP), 0.0, 0.0),
            ));
            content.add_child(&copy);
            copies.push(copy);
        }

        if !abilities.is_empty() || !buffs.is_empty() {
            let (viewport, panel) = build_panel(abilities, buffs);
            let mut vp = viewport.clone();
            content.add_child(&vp);
            let mut mat = StandardMaterial3D::new_gd();
            mat.set_shading_mode(ShadingMode::UNSHADED);
            mat.set_transparency(Transparency::ALPHA);
            mat.set_texture(TextureParam::ALBEDO, &vp.get_texture().expect("SubViewport hat eine Textur"));
            mat.set_render_priority(3);
            let mut mesh = QuadMesh::new_gd();
            mesh.set_material(&mat);
            let mut quad = MeshInstance3D::new_alloc();
            quad.set_mesh(&mesh);
            quad.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            content.add_child(&quad);
            vp.set_update_mode(UpdateMode::ALWAYS);
            self.viewport = Some(viewport);
            self.panel = Some(panel);
            self.quad = Some(quad);
        }
        self.base_mut().add_child(&content);
        // erst jetzt im Baum (Kollision wird beim Aufbau gesetzt): nicht anklickbar machen
        for copy in copies.iter_mut() {
            copy.set_collision_layer(0);
        }
        self.content = Some(content);
    }

    /// Tafelgröße an das Layout anpassen und oben bündig neben die Karte setzen.
    fn fit_panel(&mut self, card_w: f32) {
        let (Some(panel), Some(vp), Some(quad)) = (self.panel.clone(), self.viewport.as_mut(), self.quad.as_mut()) else { return };
        let min = panel.get_combined_minimum_size();
        let px = Vector2i::new(PANEL_PX, (min.y.ceil() as i32).max(8));
        if vp.get_size() != px {
            vp.set_size(px);
            let mut p = panel;
            p.set_size(Vector2::new(PANEL_PX as f32, px.y as f32));
        }
        let size = Vector2::new(px.x as f32, px.y as f32) * WORLD_PER_PX;
        if let Some(mut mesh) = quad.get_mesh().and_then(|m| m.try_cast::<QuadMesh>().ok()) {
            if mesh.get_size() != size {
                mesh.set_size(size);
            }
        }
        let dir = if self.right_side { -1.0 } else { 1.0 };
        let cards = self.shown_cards.max(1) as f32;
        let x = dir * (card_w * 0.5 + (cards - 1.0) * (card_w + GAP) + GAP + size.x * 0.5);
        let y = CARD_SHOWN_HEIGHT * 0.5 - size.y * 0.5;
        quad.set_position(Vector3::new(x, y, 0.004));
    }
}

/// Tafel als Godot-Controls in einem durchsichtigen SubViewport.
fn build_panel(abilities: &[InspectorEntry], buffs: &[InspectorEntry]) -> (Gd<SubViewport>, Gd<PanelContainer>) {
    let mut vp = SubViewport::new_alloc();
    vp.set_transparent_background(true);
    vp.set_size(Vector2i::new(PANEL_PX, 200));
    vp.set_disable_3d(true);

    let mut panel = PanelContainer::new_alloc();
    panel.set_mouse_filter(MouseFilter::IGNORE);
    let mut bg = StyleBoxFlat::new_gd();
    bg.set_bg_color(Color::from_rgba(0.09, 0.09, 0.11, 0.93));
    bg.set_corner_radius_all(16);
    bg.set_content_margin_all(18.0);
    bg.set_shadow_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    panel.add_theme_stylebox_override("panel", &bg);
    panel.set_custom_minimum_size(Vector2::new(PANEL_PX as f32, 0.0));

    let mut list = VBoxContainer::new_alloc();
    list.add_theme_constant_override("separation", 16);
    for a in abilities {
        list.add_child(&entry(&a.title, &a.text, true));
    }
    if !buffs.is_empty() {
        for b in buffs {
            list.add_child(&entry(&b.title, &b.text, false));
        }
    }
    panel.add_child(&list);
    vp.add_child(&panel);
    panel.set_anchors_preset(LayoutPreset::TOP_LEFT);
    (vp, panel)
}

/// Ein Eintrag: Symbol links, rechts Name (fett, groß, weiß) und Erklärung (kursiv, hellgrau).
fn entry(title: &str, text: &str, ability: bool) -> Gd<Control> {
    let mut row = HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 14);
    row.add_child(&icon(title, ability));
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 2);
    col.set_h_size_flags(godot::classes::control::SizeFlags::EXPAND_FILL);
    let mut name = Label::new_alloc();
    name.set_text(title);
    name.add_theme_font_override("font", &font(true, false));
    name.add_theme_font_size_override("font_size", TITLE_PX);
    name.add_theme_color_override("font_color", Color::from_rgb(0.97, 0.97, 0.97));
    col.add_child(&name);
    if !text.is_empty() {
        // Erklärung mit eingebetteten Manasymbolen („{T}: Add {G}.“)
        let mut desc = godot::classes::RichTextLabel::new_alloc();
        desc.set_fit_content(true);
        desc.set_scroll_active(false);
        desc.set_autowrap_mode(AutowrapMode::WORD_SMART);
        desc.add_theme_font_override("normal_font", &font(false, true));
        desc.add_theme_font_size_override("normal_font_size", TEXT_PX);
        desc.add_theme_color_override("default_color", Color::from_rgb(0.86, 0.86, 0.86));
        desc.set_custom_minimum_size(Vector2::new(PANEL_PX as f32 - 36.0 - ICON_PX - 14.0, 0.0));
        crate::game::mana_symbols::fill_rich(&mut desc, text, TEXT_PX);
        col.add_child(&desc);
    }
    row.add_child(&col);
    row.upcast()
}

/// Symbol vor einem Eintrag: weißes Abzeichen mit dem Anfangsbuchstaben (Fähigkeit) bzw. Plus
/// (Veränderung). Eigene Symbol-Grafiken je Schlüsselwort können hier später eingesetzt werden.
fn icon(title: &str, ability: bool) -> Gd<Control> {
    let mut wrap = MarginContainer::new_alloc();
    wrap.set_custom_minimum_size(Vector2::new(ICON_PX, ICON_PX));
    let mut badge = PanelContainer::new_alloc();
    let mut style = StyleBoxFlat::new_gd();
    style.set_bg_color(Color::from_rgba(0.0, 0.0, 0.0, 0.0));
    style.set_border_color(Color::from_rgb(0.95, 0.95, 0.95));
    style.set_border_width_all(3);
    style.set_corner_radius_all(if ability { 23 } else { 8 });
    badge.add_theme_stylebox_override("panel", &style);
    badge.set_custom_minimum_size(Vector2::new(ICON_PX, ICON_PX));
    badge.set_v_size_flags(godot::classes::control::SizeFlags::SHRINK_BEGIN);
    let mut l = Label::new_alloc();
    l.set_text(&if ability { title.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default() } else { "+".to_string() });
    l.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
    l.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    l.add_theme_font_override("font", &font(true, false));
    l.add_theme_font_size_override("font_size", 26);
    l.add_theme_color_override("font_color", Color::from_rgb(0.97, 0.97, 0.97));
    badge.add_child(&l);
    wrap.add_child(&badge);
    wrap.upcast()
}

/// Schrift: Serifenschrift des Systems, fett bzw. kursiv (schräg gestellt).
fn font(bold: bool, italic: bool) -> Gd<FontVariation> {
    let mut base = SystemFont::new_gd();
    base.set_font_names(&PackedStringArray::from(&[GString::from("Noto Serif"), GString::from("DejaVu Serif"), GString::from("serif")]));
    base.set_font_weight(if bold { 700 } else { 400 });
    let mut v = FontVariation::new_gd();
    v.set_base_font(&base);
    if italic {
        v.set_variation_transform(Transform2D::from_cols(Vector2::new(1.0, 0.0), Vector2::new(0.2, 1.0), Vector2::ZERO));
    }
    v
}
