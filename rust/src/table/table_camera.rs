//! TableCamera: wechselt weich zwischen Tischübersicht, Nahaufnahme einer Karte
//! und einem leichten Mitführen während des Ziehens.
//!
//! **Zwei Ebenen:** Die Kamera selbst zeichnet nur die Kulisse ([`BACKGROUND_VISUAL_LAYER`]).
//! Alles andere (Karten, Hand, Bühnen, 3D-Knöpfe, Pfeile, Effekte) zeichnet eine zweite Kamera
//! mit gleicher Lage in einem durchsichtigen `SubViewport` auf der Bildebene
//! [`FOREGROUND_CANVAS_LAYER`] – über den Medaillons und Namen ([`SEAT_CANVAS_LAYER`]), unter
//! dem übrigen HUD. Beide teilen sich die Welt; Mausstrahlen gehen weiter von dieser Kamera aus.

use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{Camera3D, CanvasLayer, ICamera3D, SubViewport, SubViewportContainer};
use godot::prelude::*;

use super::layers::{ALL_VISUAL_LAYERS, BACKGROUND_VISUAL_LAYER, FOREGROUND_CANVAS_LAYER};

#[derive(Clone)]
enum CameraMode {
    Overview,
    /// Nahaufnahme: Kamera schaut senkrecht auf die Kartenvorderseite.
    Focus(Gd<Node3D>),
    /// Während des Ziehens: Kamera folgt der Karte ein Stück und zoomt leicht.
    Follow(Gd<Node3D>),
}

/// Tischkamera: Übersicht über den Tisch, Nahaufnahme einer Karte (Rechtsklick) und leichtes
/// Mitführen beim Ziehen einer Karte. Alle Übergänge sind weich.
#[derive(GodotClass)]
#[class(base=Camera3D)]
pub struct TableCamera {
    /// Kameraposition in der Übersicht.
    #[export]
    overview_position: Vector3,
    /// Blickpunkt in der Übersicht.
    #[export]
    overview_target: Vector3,
    /// Abstand zur Karte in der Nahaufnahme.
    #[export]
    focus_distance: f32,
    /// Wie stark die Kamera einer gezogenen Karte folgt (0 = gar nicht, 1 = voll).
    #[export(range = (0.0, 1.0))]
    follow_strength: f32,
    /// Abstand zur gezogenen Karte, wenn follow_strength = 1.
    #[export]
    follow_distance: f32,
    /// Glättung der Kamerabewegung (größer = schneller).
    #[export]
    smoothing: f32,

    mode: CameraMode,
    /// Vordergrund-Kamera (im durchsichtigen SubViewport über den Medaillons).
    foreground: Option<Gd<Camera3D>>,
    base: Base<Camera3D>,
}

#[godot_api]
impl ICamera3D for TableCamera {
    fn init(base: Base<Camera3D>) -> Self {
        Self {
            overview_position: Vector3::new(0.0, 6.2, 5.2),
            overview_target: Vector3::new(0.0, 0.0, 0.7),
            focus_distance: 1.25,
            follow_strength: 0.3,
            follow_distance: 3.5,
            smoothing: 6.0,
            mode: CameraMode::Overview,
            foreground: None,
            base,
        }
    }

    fn ready(&mut self) {
        let xf = self.overview_transform();
        self.base_mut().set_global_transform(xf);
        self.build_foreground();
    }

    fn process(&mut self, delta: f64) {
        // Freigegebene Karten sicher behandeln.
        if let CameraMode::Focus(n) | CameraMode::Follow(n) = &self.mode {
            if !n.is_instance_valid() {
                self.mode = CameraMode::Overview;
            }
        }

        let desired = match &self.mode {
            CameraMode::Overview => self.overview_transform(),
            CameraMode::Focus(node) => self.focus_transform(&node.get_global_transform()),
            CameraMode::Follow(node) => self.follow_transform(node.get_global_position()),
        };

        let w = 1.0 - (-self.smoothing * delta as f32).exp();
        let current = self.base().get_global_transform();
        let close = |a: &Transform3D| {
            (a.origin - desired.origin).length() < 1e-4
                && (a.basis.col_a() - desired.basis.col_a()).length() < 1e-5
                && (a.basis.col_b() - desired.basis.col_b()).length() < 1e-5
                && (a.basis.col_c() - desired.basis.col_c()).length() < 1e-5
        };
        if close(&current) {
            // in Ruhe: nichts setzen (die Vordergrund-Kamera steht schon gleich)
            return;
        }
        let mut next = current.interpolate_with(&desired, w);
        // am Ziel einrasten, sonst nähert sich die Kamera endlos in winzigen Schritten an
        if close(&next) {
            next = desired;
        }
        self.base_mut().set_global_transform(next);
        self.sync_foreground();
    }
}

#[godot_api]
impl TableCamera {
    /// Zurück zur Tischübersicht.
    #[func]
    pub fn show_overview(&mut self) {
        self.mode = CameraMode::Overview;
    }

    /// Nahaufnahme einer Karte (oder eines beliebigen Node3D).
    #[func]
    pub fn focus_on(&mut self, node: Gd<Node3D>) {
        self.mode = CameraMode::Focus(node);
    }

    /// Leichtes Mitführen (z. B. während eine Karte gezogen wird).
    #[func]
    pub fn follow(&mut self, node: Gd<Node3D>) {
        self.mode = CameraMode::Follow(node);
    }

    /// Ist die Kamera gerade auf genau diesen Node fokussiert?
    #[func]
    pub fn is_focused_on(&self, node: Gd<Node3D>) -> bool {
        matches!(&self.mode, CameraMode::Focus(n) if n == &node)
    }

    /// Ist die Kamera gerade in der Übersicht?
    #[func]
    pub fn is_overview(&self) -> bool {
        matches!(self.mode, CameraMode::Overview)
    }
}

impl TableCamera {
    fn overview_transform(&self) -> Transform3D {
        look_from(self.overview_position, self.overview_target, Vector3::UP)
    }

    fn focus_transform(&self, card: &Transform3D) -> Transform3D {
        let center = card.origin;
        let mut normal = card.basis.col_b().normalized();
        // Immer von der Seite schauen, auf der die Übersichtskamera steht.
        if normal.dot(self.overview_position - center) < 0.0 {
            normal = -normal;
        }
        // Kartenoberkante zeigt lokal nach -Z -> als "oben" im Bild verwenden.
        let mut up = -card.basis.col_c().normalized();
        if up.cross(normal).length_squared() < 1e-4 {
            up = Vector3::FORWARD;
        }
        // Minimal schräg von unten, wirkt plastischer als exakt senkrecht.
        let dir = (normal * 0.96 - up * 0.28).normalized();
        look_from(center + dir * self.focus_distance, center, up)
    }

    fn follow_transform(&self, card_pos: Vector3) -> Transform3D {
        let k = self.follow_strength;
        let view_dir = (self.overview_position - self.overview_target).normalized();
        let near_pos = card_pos + view_dir * self.follow_distance;
        let pos = self.overview_position.lerp(near_pos, k);
        let target = self.overview_target.lerp(card_pos, k);
        look_from(pos, target, Vector3::UP)
    }
}

fn look_from(eye: Vector3, target: Vector3, up: Vector3) -> Transform3D {
    let dir = target - eye;
    if dir.length_squared() < 1e-6 {
        return Transform3D::new(Basis::IDENTITY, eye);
    }
    Transform3D::new(Basis::looking_at_ex(dir).up(up).done(), eye)
}

impl TableCamera {
    /// Vordergrund-Ebene anlegen: Bildebene [`FOREGROUND_CANVAS_LAYER`] mit einem durchsichtigen
    /// `SubViewport` (gleiche Welt, gleiche Bildeinstellungen wie das Hauptbild) und einer
    /// Kamera, die alles außer der Kulisse zeichnet. Die Ebene lässt Mausklicks durch.
    fn build_foreground(&mut self) {
        let Some(root) = self.base().get_viewport() else { return };
        self.base_mut().set_cull_mask(BACKGROUND_VISUAL_LAYER);

        let mut layer = CanvasLayer::new_alloc();
        layer.set_name("Foreground3D");
        layer.set_layer(FOREGROUND_CANVAS_LAYER);
        let mut container = SubViewportContainer::new_alloc();
        container.set_stretch(true);
        container.set_anchors_preset(LayoutPreset::FULL_RECT);
        container.set_mouse_filter(MouseFilter::IGNORE);
        // das Bild der Ebene ist vormultipliziert (durchsichtiger Grund): so überlagern
        let mut blend = godot::classes::CanvasItemMaterial::new_gd();
        blend.set_blend_mode(godot::classes::canvas_item_material::BlendMode::PREMULT_ALPHA);
        container.set_material(&blend);
        let mut vp = SubViewport::new_alloc();
        vp.set_transparent_background(true);
        vp.set_handle_input_locally(false);
        vp.set_disable_input(true);
        vp.set_msaa_3d(root.get_msaa_3d());
        vp.set_screen_space_aa(root.get_screen_space_aa());
        vp.set_use_taa(root.is_using_taa());
        vp.set_use_debanding(root.is_using_debanding());
        vp.set_scaling_3d_mode(root.get_scaling_3d_mode());
        vp.set_scaling_3d_scale(root.get_scaling_3d_scale());
        vp.set_positional_shadow_atlas_size(root.get_positional_shadow_atlas_size());
        let mut cam = Camera3D::new_alloc();
        cam.set_cull_mask(ALL_VISUAL_LAYERS & !BACKGROUND_VISUAL_LAYER);
        vp.add_child(&cam);
        container.add_child(&vp);
        layer.add_child(&container);
        cam.set_current(true);
        self.foreground = Some(cam);
        // beim Laden der Szene baut der Elternknoten noch seine Kinder auf
        self.base_mut().call_deferred("add_child", &[layer.to_variant()]);
        self.sync_foreground();
    }

    /// Vordergrund-Kamera auf Lage und Optik dieser Kamera setzen (jedes Frame nach dem Bewegen).
    fn sync_foreground(&mut self) {
        let Some(mut cam) = self.foreground.clone().filter(|c| c.is_instance_valid()) else { return };
        let me = self.base();
        cam.set_global_transform(me.get_global_transform());
        cam.set_fov(me.get_fov());
        cam.set_near(me.get_near());
        cam.set_far(me.get_far());
        cam.set_h_offset(me.get_h_offset());
        cam.set_v_offset(me.get_v_offset());
        cam.set_keep_aspect_mode(me.get_keep_aspect_mode());
        cam.set_environment(me.get_environment().as_ref());
    }
}
