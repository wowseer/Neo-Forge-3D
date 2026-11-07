//! SeatBadge: rundes Helden-Medaillon eines Spielers für das HUD (Vorlage: Video „Blocken“).
//!
//! Runder Porträt-Rahmen aus Metall (Gold, von links oben beleuchtet) mit einer Figur in der
//! Farbe der Seite (Kapuze, Gesicht im Schatten, leuchtende Augen) bzw. einem eigenen Bild
//! ([member SeatBadge.portrait]). Unten überlappt eine dunkle Plakette mit dem Lebenspunktestand
//! den Rahmen, links und rechts davon zwei Metall-Klingen. Wer am Zug ist, bekommt einen
//! goldenen Strahlenkranz; Lebensänderungen blitzen kurz auf (rot = Schaden, grün = Heilung).
//! Ist der Spieler ein gültiges Ziel, pulsiert ein goldener Ring und das Medaillon ist
//! anklickbar (Signal `clicked`). Das ruhende Medaillon zeichnet `draw()` nur bei Änderungen,
//! Zielring, Strahlen und Augen jedes Bild in eigene Ebenen; die Größe folgt der
//! Fensterhöhe (Maße für 1080 Pixel).

use std::f32::consts::{PI, TAU};

use godot::classes::control::MouseFilter;
use godot::classes::{
    Control, Font, Geometry2D, IControl, InputEvent, InputEventMouseButton, RenderingServer, SystemFont, Texture2D,
};
use godot::global::HorizontalAlignment;
use godot::global::MouseButton;
use godot::prelude::*;

use super::tri_batch::TriBatch;

/// Maße bei 1080 Pixel Fensterhöhe: Außenradius des Rahmens, Breite des Metallrings, Abstand
/// des Ringmittelpunkts vom oberen Rand und Größe der Lebens-Plakette.
const RING_R: f32 = 74.0;
const RING_W: f32 = 11.0;
const TOP: f32 = 24.0;
const PLATE_W: f32 = 96.0;
const PLATE_H: f32 = 54.0;
/// Die Plakette sitzt so weit unter der Ringmitte (überlappt den unteren Rand).
const PLATE_DROP: f32 = RING_R - 6.0;
/// Spannweite der Klingen links und rechts (Spitze ab Mitte).
const WING_REACH: f32 = 106.0;
/// Schriftgröße der Lebenspunkte.
const LIFE_FONT: f32 = 40.0;
/// Dauer des Aufblitzens nach einer Lebensänderung (Sekunden).
const FLASH_TIME: f32 = 0.6;

const GOLD_LIGHT: Color = Color::from_rgb(1.0, 0.92, 0.66);
const GOLD_DARK: Color = Color::from_rgb(0.42, 0.27, 0.1);
const SILVER_LIGHT: Color = Color::from_rgb(0.96, 0.97, 1.0);
const SILVER_DARK: Color = Color::from_rgb(0.33, 0.35, 0.42);
const PLATE_TOP: Color = Color::from_rgb(0.25, 0.25, 0.28);
const PLATE_BOTTOM: Color = Color::from_rgb(0.07, 0.07, 0.09);
const TEXT: Color = Color::from_rgb(0.98, 0.97, 0.94);

/// Rundes Helden-Medaillon (HUD): Figur in der Seitenfarbe [member accent] (oder [member portrait])
/// im Metallrahmen, darunter eine Plakette mit den Lebenspunkten [member life]. Ist [member active]
/// gesetzt (Spieler am Zug), dreht sich ein goldener Strahlenkranz dahinter; Lebensänderungen
/// blitzen kurz rot (Schaden) bzw. grün (Heilung). Wird vom [TurnHud] jedes Frame über
/// [method set_state] aktualisiert und mit der Unterkante (Spieler) bzw. Oberkante (Gegner) an den
/// Anker `HeroAnchor` des Sitzplatzes gesetzt.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct SeatBadge {
    /// Name der Seite (für Lebensänderungen: erst ab dem ersten Zustand blitzt die Plakette).
    #[var]
    seat_name: GString,
    /// Angezeigte Lebenspunkte.
    #[var]
    life: i64,
    /// Ist diese Seite am Zug? (goldener Strahlenkranz)
    #[var]
    active: bool,
    /// Farbe der Figur (Seitenfarbe, z. B. blau für den Spieler, rot für den Gegner).
    #[export]
    accent: Color,
    /// Eigenes Porträt (rund zugeschnitten) statt der gezeichneten Figur; leer = Figur.
    #[export]
    portrait: Option<Gd<Texture2D>>,
    /// Ist dieser Spieler gerade ein gültiges Ziel? Dann pulsiert ein goldener Ring und ein Klick
    /// löst [signal clicked] aus (sonst lässt das Medaillon Mausklicks durch).
    #[var(get = is_targetable, set = set_targetable)]
    targetable: bool,
    /// Ist dieser Spieler als Ziel gewählt? (heller, ruhiger Ring)
    #[var(pub)]
    target_selected: bool,

    time: f32,
    /// Restdauer des Aufblitzens nach einer Lebensänderung (Sekunden) und dessen Farbe.
    flash: f32,
    flash_color: Color,
    /// Maßstab (Fensterhöhe / 1080).
    k: f32,
    life_font: Option<Gd<Font>>,
    /// Animierte Ebenen (eigene CanvasItems, Kinder des Medaillons): hinten Zielring und
    /// Strahlenkranz, vorn die Augen der Figur. Nur sie werden jedes Bild neu gezeichnet, das
    /// übrige Medaillon nur, wenn sich daran etwas ändert (siehe [`StaticLook`]).
    layers: Option<(Rid, Rid)>,
    /// Stand, mit dem das ruhende Medaillon zuletzt gezeichnet wurde.
    drawn: Option<StaticLook>,
    base: Base<Control>,
}

/// Alles, wovon das ruhende Medaillon (Klingen, Porträt, Ring, Figur, Plakette) abhängt: Ändert
/// sich etwas davon, wird es neu gezeichnet.
#[derive(Clone, PartialEq)]
struct StaticLook {
    life: i64,
    accent: Color,
    portrait: Option<InstanceId>,
    k: f32,
    flash: f32,
    size: Vector2,
}

#[godot_api]
impl IControl for SeatBadge {
    fn init(base: Base<Control>) -> Self {
        Self {
            seat_name: GString::new(),
            life: 0,
            active: false,
            accent: Color::from_rgb(0.25, 0.55, 0.95),
            portrait: None,
            targetable: false,
            target_selected: false,
            time: 0.0,
            flash: 0.0,
            flash_color: Color::WHITE,
            k: 0.0,
            life_font: None,
            layers: None,
            drawn: None,
            base,
        }
    }

    fn enter_tree(&mut self) {
        let mut rs = RenderingServer::singleton();
        let me = self.base().get_canvas_item();
        let behind = rs.canvas_item_create();
        let front = rs.canvas_item_create();
        for layer in [behind, front] {
            rs.canvas_item_set_parent(layer, me);
            // Aussparung der Medaillon-Maske ([CardCutout]) gilt auch hier
            rs.canvas_item_set_use_parent_material(layer, true);
        }
        rs.canvas_item_set_draw_behind_parent(behind, true);
        self.layers = Some((behind, front));
    }

    fn exit_tree(&mut self) {
        if let Some((behind, front)) = self.layers.take() {
            let mut rs = RenderingServer::singleton();
            rs.free_rid(behind);
            rs.free_rid(front);
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        // Lebenspunkte in einer kräftigen Serifenschrift
        let mut font = SystemFont::new_gd();
        font.set_font_names(&PackedStringArray::from(&[
            GString::from("Noto Serif"),
            GString::from("DejaVu Serif"),
            GString::from("serif"),
        ]));
        font.set_font_weight(800);
        self.life_font = Some(font.upcast());
        self.update_scale();
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        let Ok(mb) = event.try_cast::<InputEventMouseButton>() else { return };
        if self.targetable && mb.is_pressed() && mb.get_button_index() == MouseButton::LEFT {
            self.signals().clicked().emit();
            self.base_mut().accept_event();
        }
    }

    fn process(&mut self, delta: f64) {
        self.time += delta as f32;
        self.flash = (self.flash - delta as f32).max(0.0);
        self.update_scale();
        if self.drawn.as_ref() != Some(&self.static_look()) {
            self.base_mut().queue_redraw();
        }
        self.draw_layers();
    }

    /// Ruhendes Medaillon: Klingen, Porträt, Ring, Figur und Plakette (Zielring, Strahlen und
    /// Augen zeichnet `draw_layers` jedes Bild in eigene Ebenen).
    fn draw(&mut self) {
        self.drawn = Some(self.static_look());
        let (c, r, k) = self.geometry();

        // Flächen gesammelt zeichnen (ein Befehl statt hunderter, siehe TriBatch)
        let mut batch = TriBatch::new(self.base().get_canvas_item());
        self.draw_wings(&mut batch, c + Vector2::new(0.0, PLATE_DROP * k), k);
        self.draw_portrait(&mut batch, c, r - RING_W * k);
        self.draw_ring(&mut batch, c, r, k);
        if self.portrait.is_none() {
            self.draw_figure(&mut batch, c, r - RING_W * k);
        }
        self.draw_plate(&mut batch, c + Vector2::new(0.0, PLATE_DROP * k), k);
    }
}

#[godot_api]
impl SeatBadge {
    /// Das Medaillon wurde angeklickt, während [member targetable] gesetzt war.
    #[signal]
    fn clicked();

    /// Getter für [member targetable].
    #[func]
    pub fn is_targetable(&self) -> bool {
        self.targetable
    }

    /// Setter für [member targetable]: nur ein wählbares Medaillon fängt Mausklicks ab.
    #[func]
    pub fn set_targetable(&mut self, value: bool) {
        self.targetable = value;
        self.base_mut().set_mouse_filter(if value { MouseFilter::STOP } else { MouseFilter::IGNORE });
    }

    /// Setzt Name, Lebenspunkte und „am Zug“ auf einmal. Weicht `life` vom bisherigen Wert ab,
    /// blitzt die Plakette kurz auf (rot bei Schaden, grün bei Heilung).
    #[func]
    pub fn set_state(&mut self, seat_name: GString, life: i64, active: bool) {
        if life != self.life && !self.seat_name.is_empty() {
            self.flash = FLASH_TIME;
            self.flash_color = if life < self.life { Color::from_rgb(0.85, 0.12, 0.1) } else { Color::from_rgb(0.2, 0.75, 0.3) };
        }
        self.seat_name = seat_name;
        self.life = life;
        self.active = active;
    }
}

impl SeatBadge {
    /// Mittelpunkt und Außenradius des Rings sowie Maßstab.
    fn geometry(&self) -> (Vector2, f32, f32) {
        let k = self.k.max(0.1);
        let size = self.base().get_size();
        (Vector2::new(size.x * 0.5, (TOP + RING_R) * k), RING_R * k, k)
    }

    fn static_look(&self) -> StaticLook {
        StaticLook {
            life: self.life,
            accent: self.accent,
            portrait: self.portrait.as_ref().map(|p| p.instance_id()),
            k: self.k,
            flash: self.flash,
            size: self.base().get_size(),
        }
    }

    /// Animierte Ebenen neu zeichnen: hinten Zielring (pulsierend = wählbar, ruhig und hell =
    /// gewählt) und Strahlenkranz (am Zug), vorn die flackernden Augen der Figur.
    fn draw_layers(&mut self) {
        let Some((behind, front)) = self.layers else { return };
        if !self.base().is_visible_in_tree() {
            return;
        }
        let mut rs = RenderingServer::singleton();
        rs.canvas_item_clear(behind);
        rs.canvas_item_clear(front);
        let (c, r, k) = self.geometry();
        if self.targetable || self.target_selected {
            let pulse = 0.5 + 0.5 * (self.time * 4.0).sin();
            let (alpha, grow) = if self.target_selected { (0.95, 9.0) } else { (0.45 + 0.4 * pulse, 6.0 + 4.0 * pulse) };
            let ring = Color::from_rgba(1.0, 0.82, 0.35, alpha);
            // wie draw_arc: 72 Punkte, der letzte schließt den Kreis
            let pts: Vec<Vector2> = (0..72).map(|i| c + Vector2::from_angle(i as f32 / 71.0 * TAU) * (r + grow * k)).collect();
            rs.canvas_item_add_polyline_ex(behind, &PackedVector2Array::from(pts.as_slice()), &PackedColorArray::from(&[ring]))
                .width(5.0 * k)
                .antialiased(true)
                .done();
        }
        if self.active {
            let mut batch = TriBatch::new(behind);
            self.draw_rays(&mut batch, c, r);
        }
        if self.portrait.is_none() {
            let mut batch = TriBatch::new(front);
            self.draw_eyes(&mut batch, c, r - RING_W * k);
        }
    }

    /// Maßstab aus der Fensterhöhe; ändert er sich, wächst bzw. schrumpft die Mindestgröße mit.
    fn update_scale(&mut self) {
        let h = self.base().get_viewport_rect().size.y;
        let k = if h > 0.0 { h / 1080.0 } else { 1.0 };
        if (k - self.k).abs() > 1e-3 {
            self.k = k;
            let size = Vector2::new(2.0 * (WING_REACH + 6.0), TOP + PLATE_DROP + RING_R + PLATE_H * 0.5 + 6.0) * k;
            self.base_mut().set_custom_minimum_size(size);
        }
    }

    /// Goldener Strahlenkranz (am Zug): weicher Schein und sich langsam drehende Strahlen.
    fn draw_rays(&self, batch: &mut TriBatch, c: Vector2, r: f32) {
        let pulse = 0.75 + 0.25 * (self.time * 2.5).sin();
        for i in 0..8 {
            let t = i as f32 / 8.0;
            let a = 0.09 * pulse;
            batch.circle(c + Vector2::new(0.0, -0.2 * r * t), r * (1.6 - 0.6 * t), Color::from_rgba(1.0, 0.86, 0.5 + 0.3 * t, a));
        }
        let spin = self.time * 0.2;
        let n = 28;
        for i in 0..n {
            let a = spin + i as f32 * TAU / n as f32;
            let len = r * [2.1, 1.5, 1.8, 1.35][i % 4] * (0.88 + 0.12 * (self.time * 1.7 + i as f32).sin());
            let half = 0.045;
            let pts = [c, c + Vector2::from_angle(a - half) * len, c + Vector2::from_angle(a + half) * len];
            let core = Color::from_rgba(1.0, 0.93, 0.7, 0.75 * pulse);
            let tip = Color::from_rgba(1.0, 0.8, 0.4, 0.0);
            batch.polygon(&pts, &[core, tip, tip]);
        }
    }

    /// Zwei Klingen je Seite auf Höhe der Plakette: hinten Gold, vorn Silber.
    fn draw_wings(&mut self, batch: &mut TriBatch, p: Vector2, k: f32) {
        for side in [-1.0f32, 1.0] {
            let at = |x: f32, y: f32| p + Vector2::new(side * x, y) * k;
            // hintere, goldene Klinge (etwas tiefer, schräg nach unten)
            let back = [at(30.0, -6.0), at(WING_REACH - 6.0, 14.0), at(74.0, 18.0), at(34.0, 20.0)];
            let gold = [GOLD_LIGHT, GOLD_DARK.lerp(GOLD_LIGHT, 0.5), GOLD_DARK, GOLD_DARK];
            batch.polygon(&back, &gold);
            // vordere, silberne Klinge mit Spitze nach außen
            let front = [at(32.0, -16.0), at(WING_REACH, -2.0), at(36.0, 10.0)];
            let silver = [SILVER_LIGHT, SILVER_LIGHT.lerp(SILVER_DARK, 0.3), SILVER_DARK];
            batch.polygon(&front, &silver);
            let edge = [at(32.0, -16.0), at(WING_REACH, -2.0), at(36.0, 10.0), at(32.0, -16.0)];
            batch.flush();
            self.base_mut()
                .draw_polyline_ex(&PackedVector2Array::from(&edge), Color::from_rgba(0.1, 0.1, 0.12, 0.6))
                .width(1.2 * k)
                .antialiased(true)
                .done();
        }
    }

    /// Porträtfläche: eigenes Bild bzw. Figur mit Kapuze in der Seitenfarbe vor einem Verlauf.
    fn draw_portrait(&mut self, batch: &mut TriBatch, c: Vector2, r: f32) {
        let circle = circle_points(c, r, 64);
        if let Some(tex) = self.portrait.clone() {
            batch.flush();
            let uvs: Vec<Vector2> = circle.iter().map(|p| (*p - c) / (2.0 * r) + Vector2::splat(0.5)).collect();
            let white = vec![Color::WHITE; circle.len()];
            self.base_mut()
                .draw_polygon_ex(&PackedVector2Array::from(circle.as_slice()), &PackedColorArray::from(white.as_slice()))
                .uvs(&PackedVector2Array::from(uvs.as_slice()))
                .texture(&tex)
                .done();
            return;
        }
        let accent = self.accent;
        let shade = |f: f32| Color::from_rgba(accent.r * f, accent.g * f, accent.b * f, 1.0);
        let s = r / 55.0;
        let at = |x: f32, y: f32| c + Vector2::new(x, y) * s;

        // Hintergrund: dunkler Grund, nach oben heller (Licht hinter dem Kopf)
        batch.circle(c, r, shade(0.16));
        for i in 0..10 {
            let t = i as f32 / 10.0;
            let col = shade(0.16).lerp(accent.lerp(Color::WHITE, 0.35), (t * 0.55) as f64);
            batch.circle(at(0.0, -14.0 * t), r * (1.0 - t) * 0.95, col);
        }

        // Schultern: Ellipse unten, am Kreis abgeschnitten
        let shoulders: Vec<Vector2> = (0..40)
            .map(|i| {
                let a = i as f32 / 40.0 * TAU;
                at(0.0, 54.0) + Vector2::new(a.cos() * 58.0, a.sin() * 30.0) * s
            })
            .collect();
        let clipped = Geometry2D::singleton()
            .intersect_polygons(&PackedVector2Array::from(shoulders.as_slice()), &PackedVector2Array::from(circle.as_slice()));
        for poly in clipped.iter_shared() {
            batch.colored_polygon(poly.as_slice(), shade(0.42));
        }
    }

    /// Figur mit Kapuze vor dem Porträt-Grund; die Spitze der Kapuze ragt über den Ring hinaus
    /// (wie im Video), deshalb nach dem Ring gezeichnet.
    fn draw_figure(&mut self, batch: &mut TriBatch, c: Vector2, r: f32) {
        let accent = self.accent;
        let shade = |f: f32| Color::from_rgba(accent.r * f, accent.g * f, accent.b * f, 1.0);
        let s = r / 55.0;
        let at = |x: f32, y: f32| c + Vector2::new(x, y) * s;
        // Kapuze: Spitzbogen, oben im Licht, unten im Schatten
        let hood = [
            at(-34.0, 36.0),
            at(-33.0, 6.0),
            at(-31.0, -16.0),
            at(-25.0, -36.0),
            at(-14.0, -52.0),
            at(0.0, -63.0),
            at(14.0, -52.0),
            at(25.0, -36.0),
            at(31.0, -16.0),
            at(33.0, 6.0),
            at(34.0, 36.0),
            at(0.0, 30.0),
        ];
        let light = accent.lerp(Color::WHITE, 0.55);
        let mid = accent.lerp(Color::WHITE, 0.15);
        let dark = shade(0.45);
        let cols = [dark, mid, mid, light, light, light, light, light, mid, mid, dark, dark];
        batch.polygon(&hood, &cols);
        // Faltenlinie der Kapuze
        let fold = [at(-7.0, -50.0), at(0.0, -26.0), at(7.0, -50.0)];
        batch.flush();
        self.base_mut()
            .draw_polyline_ex(&PackedVector2Array::from(&fold), Color::from_rgba(1.0, 1.0, 1.0, 0.25))
            .width(1.5 * s)
            .antialiased(true)
            .done();

        // Rand der Kapuzenöffnung, Gesicht im Schatten, darin leuchtende Augen
        batch.ellipse(at(0.0, 1.0), 19.0 * s, 25.0 * s, shade(0.3));
        batch.ellipse(at(0.0, 0.0), 15.0 * s, 21.0 * s, Color::from_rgb(0.03, 0.03, 0.05));
    }

    /// Leuchtende, leicht flackernde Augen im Gesicht der Figur (animiert, vordere Ebene).
    fn draw_eyes(&self, batch: &mut TriBatch, c: Vector2, r: f32) {
        let s = r / 55.0;
        let at = |x: f32, y: f32| c + Vector2::new(x, y) * s;
        let glow = self.accent.lerp(Color::WHITE, 0.6);
        let flicker = 0.85 + 0.15 * (self.time * 3.1).sin();
        for x in [-6.5f32, 6.5] {
            batch.ellipse(at(x, -3.0), 6.0 * s, 3.5 * s, Color::from_rgba(glow.r, glow.g, glow.b, 0.25 * flicker));
            batch.ellipse(at(x, -3.0), 3.4 * s, 1.4 * s, Color::from_rgba(glow.r, glow.g, glow.b, flicker));
        }
    }

    /// Metallring: von links oben beleuchtet (Licht → Gold hell, unten rechts dunkel), dunkle Kanten.
    fn draw_ring(&mut self, batch: &mut TriBatch, c: Vector2, r: f32, k: f32) {
        let inner = r - RING_W * k;
        let n = 72;
        let light_dir = -0.75 * PI; // links oben
        for i in 0..n {
            let a0 = i as f32 / n as f32 * TAU;
            let a1 = (i + 1) as f32 / n as f32 * TAU;
            let lit = |a: f32| 0.5 + 0.5 * (a - light_dir).cos();
            let col = |a: f32| GOLD_DARK.lerp(GOLD_LIGHT, lit(a) as f64);
            let pts = [
                c + Vector2::from_angle(a0) * inner,
                c + Vector2::from_angle(a0) * r,
                c + Vector2::from_angle(a1) * r,
                c + Vector2::from_angle(a1) * inner,
            ];
            let cols = [col(a0 + PI) , col(a0), col(a1), col(a1 + PI)];
            batch.polygon(&pts, &cols);
        }
        batch.flush();
        let edge = Color::from_rgba(0.12, 0.08, 0.04, 0.9);
        self.base_mut().draw_arc_ex(c, r, 0.0, TAU, 72, edge).width(2.2 * k).antialiased(true).done();
        self.base_mut().draw_arc_ex(c, inner, 0.0, TAU, 72, edge).width(2.0 * k).antialiased(true).done();
        // feine Glanzlinie in der Mitte des Rings
        let shine = Color::from_rgba(1.0, 0.97, 0.85, 0.45);
        self.base_mut()
            .draw_arc_ex(c, (r + inner) * 0.5, PI * 1.05, PI * 1.45, 16, shine)
            .width(1.6 * k)
            .antialiased(true)
            .done();
    }

    /// Lebens-Plakette: dunkles, abgerundetes Schild mit hellem Rand, Zahl in Serifenschrift.
    /// Nach einer Lebensänderung färbt sie sich kurz rot bzw. grün und die Zahl springt auf.
    fn draw_plate(&mut self, batch: &mut TriBatch, p: Vector2, k: f32) {
        let t = self.flash / FLASH_TIME;
        let grow = 1.0 + 0.12 * t;
        let (w, h) = (PLATE_W * k * grow, PLATE_H * k * grow);
        let shape = rounded_rect(p, w, h, h * 0.42, 6);
        let top = PLATE_TOP.lerp(self.flash_color, (t * 0.85) as f64);
        let bottom = PLATE_BOTTOM.lerp(self.flash_color.darkened(0.5), (t * 0.85) as f64);
        let cols: Vec<Color> = shape.iter().map(|q| top.lerp(bottom, (((q.y - (p.y - h * 0.5)) / h).clamp(0.0, 1.0)) as f64)).collect();
        batch.polygon(&shape, &cols);
        let mut edge = shape.clone();
        edge.push(shape[0]);
        batch.flush();
        self.base_mut()
            .draw_polyline_ex(&PackedVector2Array::from(edge.as_slice()), SILVER_LIGHT.lerp(GOLD_DARK, 0.35))
            .width(2.2 * k)
            .antialiased(true)
            .done();

        let life = self.life.to_string();
        let mut fs = LIFE_FONT * k * (1.0 + 0.25 * t);
        if life.len() > 2 {
            fs *= 0.8;
        }
        let Some(font) = self.life_font.clone().or_else(|| self.base().get_theme_default_font()) else { return };
        let width = w * 1.4;
        let pos = p + Vector2::new(-width * 0.5, fs * 0.36);
        let size = fs.round() as i32;
        self.base_mut()
            .draw_string_outline_ex(&font, pos, life.as_str())
            .alignment(HorizontalAlignment::CENTER)
            .width(width)
            .font_size(size)
            .size((5.0 * k).max(2.0) as i32)
            .modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.8))
            .done();
        self.base_mut()
            .draw_string_ex(&font, pos, life.as_str())
            .alignment(HorizontalAlignment::CENTER)
            .width(width)
            .font_size(size)
            .modulate(TEXT)
            .done();
    }
}

/// Punkte eines Kreises um `c` mit Radius `r`.
fn circle_points(c: Vector2, r: f32, n: usize) -> Vec<Vector2> {
    (0..n).map(|i| c + Vector2::from_angle(i as f32 / n as f32 * TAU) * r).collect()
}

/// Rechteck um `c` (Breite `w`, Höhe `h`) mit runden Ecken (Radius `rad`, `steps` Punkte je Ecke).
fn rounded_rect(c: Vector2, w: f32, h: f32, rad: f32, steps: usize) -> Vec<Vector2> {
    let (hw, hh) = (w * 0.5 - rad, h * 0.5 - rad);
    let corners = [(hw, hh, 0.0), (-hw, hh, 0.5 * PI), (-hw, -hh, PI), (hw, -hh, 1.5 * PI)];
    let mut pts = Vec::new();
    for (x, y, a0) in corners {
        for i in 0..=steps {
            let a = a0 + i as f32 / steps as f32 * 0.5 * PI;
            pts.push(c + Vector2::new(x, y) + Vector2::from_angle(a) * rad);
        }
    }
    pts
}
