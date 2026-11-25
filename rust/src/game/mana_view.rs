//! ManaPoolView: Manapool als Reihe runder Symbole im HUD (unten am Brett, neben dem Medaillon).
//!
//! Je Manaart mit Mana im Pool ein Kreis in deren Farbe mit einem eigenen, schlicht gezeichneten
//! Symbol: Grün = Nadelbaum, Blau = Wassertropfen, Schwarz = Totenkopf, Weiß = Sonne,
//! Rot = Flamme, Farblos = Raute. Ab 2 Mana steht die Anzahl in einer kleinen Plakette unten
//! rechts. Kommt Mana hinzu (z. B. eine Ressource wird getappt), springt der Kreis kurz auf.
//! Beim Bezahlen bekommen einsetzbare Kreise einen pulsierenden goldenen Ring; ein Klick darauf
//! meldet die Manaart (Signal `mana_clicked`).
//! Alles per `draw()`, ohne Bilddateien.

use std::f32::consts::{PI, TAU};

use godot::classes::control::MouseFilter;
use godot::classes::{CanvasItem, Control, IControl, InputEvent, InputEventMouseButton};
use godot::global::MouseButton;
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use super::mana::ManaType;

/// Radius eines Mana-Kreises und Abstand zwischen zwei Kreisen (Pixel).
const RADIUS: f32 = 15.0;
const GAP: f32 = 7.0;
const RIM: Color = Color::from_rgb(0.9, 0.72, 0.36);
/// Dauer des Aufspringens (Sekunden).
const POP_TIME: f32 = 0.35;

/// Manapool als Reihe runder Symbole (HUD). Wird vom [TurnHud] jedes Frame über
/// [method set_amounts] gefüttert; die Breite passt sich der Anzahl der Kreise an. Beim Bezahlen
/// sind einsetzbare Kreise golden umrandet und anklickbar ([signal mana_clicked]).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct ManaPoolView {
    amounts: [u32; 6],
    /// Zeit seit dem letzten Zuwachs je Manaart (für das Aufspringen).
    pop: [f32; 6],
    /// Je Manaart: anklickbar (beim Bezahlen einsetzbar).
    clickable: [bool; 6],
    /// Laufzeit für das Pulsieren der anklickbaren Kreise.
    time: f32,
    /// Eingeschränktes Mana (in `amounts` mitgezählt): (Manaart, Menge, Art `artifact` /
    /// `creature` / `other`) – eigene Kreise mit kleinem Symbol, wofür es gilt.
    restricted: Vec<(ManaType, u32, String)>,
    base: Base<Control>,
}

#[godot_api]
impl IControl for ManaPoolView {
    fn init(base: Base<Control>) -> Self {
        Self { amounts: [0; 6], pop: [POP_TIME; 6], clickable: [false; 6], time: 0.0, restricted: Vec::new(), base }
    }

    fn ready(&mut self) {
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
    }

    fn process(&mut self, delta: f64) {
        let mut animating = false;
        for p in self.pop.iter_mut() {
            if *p < POP_TIME {
                *p = (*p + delta as f32).min(POP_TIME);
                animating = true;
            }
        }
        if self.clickable.iter().any(|c| *c) {
            self.time += delta as f32;
            animating = true;
        }
        if animating {
            self.base_mut().queue_redraw();
        }
    }

    fn gui_input(&mut self, event: Gd<InputEvent>) {
        let Ok(click) = event.try_cast::<InputEventMouseButton>() else { return };
        if !click.is_pressed() || click.get_button_index() != MouseButton::LEFT {
            return;
        }
        let pos = click.get_position();
        for (k, t) in self.shown().into_iter().enumerate() {
            if self.clickable[t.index()] && pos.distance_to(Self::center(k)) <= RADIUS + 3.0 {
                self.base_mut().accept_event();
                self.signals().mana_clicked().emit(t.index() as i64);
                return;
            }
        }
    }

    fn draw(&mut self) {
        for (k, t) in self.shown().into_iter().enumerate() {
            let c = Self::center(k);
            // Aufspringen: kurz größer, dann zurück (leichtes Überschwingen)
            let f = self.pop[t.index()] / POP_TIME;
            let scale = if f < 1.0 { 0.55 + 0.45 * f + 0.25 * (f * PI).sin() } else { 1.0 };
            self.draw_mana(t, c, RADIUS * scale);
            if self.clickable[t.index()] {
                // Einsetzbar: pulsierender goldener Ring
                let a = 0.55 + 0.45 * (self.time * 4.0).sin().abs();
                let gold = Color::from_rgba(1.0, 0.85, 0.35, a);
                self.base_mut().draw_arc_ex(c, RADIUS + 3.5, 0.0, TAU, 40, gold).width(3.0).antialiased(true).done();
            }
            let n = self.free(t);
            if n > 1 {
                self.draw_count(c + Vector2::new(RADIUS * 0.72, RADIUS * 0.72), n);
            }
        }
        // eingeschränktes Mana dahinter: eigener Kreis mit Symbol, wofür es gilt
        let first = self.shown().len();
        for (j, (t, n, only)) in self.restricted.clone().into_iter().enumerate() {
            let c = Self::center(first + j);
            self.draw_mana(t, c, RADIUS);
            self.draw_restriction(c + Vector2::new(-RADIUS * 0.72, RADIUS * 0.72), &only);
            if n > 1 {
                self.draw_count(c + Vector2::new(RADIUS * 0.72, RADIUS * 0.72), n);
            }
        }
    }
}

#[godot_api]
impl ManaPoolView {
    /// Ein anklickbares Mana-Symbol wurde angeklickt (`mana` = Manaart: 0 = Weiß, 1 = Blau,
    /// 2 = Schwarz, 3 = Rot, 4 = Grün, 5 = Farblos).
    #[signal]
    fn mana_clicked(mana: i64);

    /// Setzt die Mana-Mengen je Art (Reihenfolge Weiß, Blau, Schwarz, Rot, Grün, Farblos).
    /// Leeres Array = nichts anzeigen. Zuwachs lässt den Kreis aufspringen.
    #[func]
    pub fn set_amounts(&mut self, amounts: PackedInt64Array) {
        let mut next = [0u32; 6];
        for (i, a) in amounts.as_slice().iter().take(6).enumerate() {
            next[i] = (*a).max(0) as u32;
        }
        if next == self.amounts {
            return;
        }
        for i in 0..6 {
            if next[i] > self.amounts[i] {
                self.pop[i] = 0.0;
            }
        }
        self.amounts = next;
        self.update_size();
    }
}

impl ManaPoolView {
    /// Setzt, welche Manaarten anklickbar sind (Reihenfolge wie [`ManaType::ALL`]); mit
    /// anklickbaren Kreisen fängt die Anzeige Mausklicks ab, sonst lässt sie sie durch.
    pub fn set_clickable(&mut self, clickable: &[bool]) {
        let mut next = [false; 6];
        for (i, c) in clickable.iter().take(6).enumerate() {
            next[i] = *c;
        }
        if next == self.clickable {
            return;
        }
        self.clickable = next;
        let any = next.iter().any(|c| *c);
        let mut b = self.base_mut();
        b.set_mouse_filter(if any { MouseFilter::STOP } else { MouseFilter::IGNORE });
        b.set_default_cursor_shape(if any {
            godot::classes::control::CursorShape::POINTING_HAND
        } else {
            godot::classes::control::CursorShape::ARROW
        });
        b.queue_redraw();
    }

    /// Eingeschränktes Mana setzen (in den Mengen von [method set_amounts] mitgezählt): es
    /// bekommt eigene Kreise mit einem kleinen Symbol (Zahnrad = nur für Artefakte, Kralle = nur
    /// für Kreaturen, Schloss = sonst eingeschränkt), z. B. das übrige Mana von Mishra's Workshop.
    pub fn set_restricted(&mut self, restricted: &[(ManaType, u32, String)]) {
        let next: Vec<(ManaType, u32, String)> = restricted.iter().filter(|r| r.1 > 0).cloned().collect();
        if next == self.restricted {
            return;
        }
        self.restricted = next;
        self.update_size();
    }

    /// Freies (nicht eingeschränktes) Mana der Art `t`.
    fn free(&self, t: ManaType) -> u32 {
        let bound: u32 = self.restricted.iter().filter(|r| r.0 == t).map(|r| r.1).sum();
        self.amounts[t.index()].saturating_sub(bound)
    }

    /// Breite an die Zahl der Kreise anpassen und neu zeichnen.
    fn update_size(&mut self) {
        let count = (self.shown().len() + self.restricted.len()) as f32;
        let width = if count > 0.0 { count * 2.0 * RADIUS + (count - 1.0) * GAP + 4.0 } else { 0.0 };
        self.base_mut().set_custom_minimum_size(Vector2::new(width, 2.0 * RADIUS + 4.0));
        self.base_mut().queue_redraw();
    }

    /// Kleines Symbol unten links an einem Kreis: wofür das Mana gilt.
    fn draw_restriction(&mut self, c: Vector2, only: &str) {
        let r = 8.0;
        let ink = Color::from_rgb(0.95, 0.85, 0.55);
        let mut b = self.base_mut();
        b.draw_circle(c, r, Color::from_rgba(0.05, 0.05, 0.07, 0.95));
        b.draw_arc_ex(c, r, 0.0, TAU, 20, RIM).width(1.2).antialiased(true).done();
        match only {
            // Zahnrad (Artefakt): Ring mit sechs Zähnen
            "artifact" => {
                for i in 0..6 {
                    let a = i as f32 * TAU / 6.0;
                    let d = Vector2::new(a.cos(), a.sin());
                    b.draw_line_ex(c + d * 2.5, c + d * 5.6, ink).width(2.2).antialiased(true).done();
                }
                b.draw_circle(c, 3.4, ink);
                b.draw_circle(c, 1.4, Color::from_rgb(0.05, 0.05, 0.07));
            }
            // Kralle (Kreatur): drei Striche
            "creature" => {
                for dx in [-2.6f32, 0.0, 2.6] {
                    b.draw_line_ex(c + Vector2::new(dx - 1.0, -4.0), c + Vector2::new(dx + 1.0, 4.0), ink).width(1.6).antialiased(true).done();
                }
            }
            // Schloss (sonst eingeschränkt)
            _ => {
                b.draw_arc_ex(c + Vector2::new(0.0, -1.0), 2.6, PI, TAU, 10, ink).width(1.4).antialiased(true).done();
                b.draw_rect(Rect2::new(c + Vector2::new(-3.5, -1.0), Vector2::new(7.0, 5.0)), ink);
            }
        }
    }

    /// Manaarten mit freiem Mana im Pool, in Anzeigereihenfolge.
    fn shown(&self) -> Vec<ManaType> {
        ManaType::ALL.into_iter().filter(|t| self.free(*t) > 0).collect()
    }

    /// Mitte des `k`-ten Kreises.
    fn center(k: usize) -> Vector2 {
        Vector2::new(RADIUS + 2.0 + k as f32 * (2.0 * RADIUS + GAP), RADIUS + 2.0)
    }

    /// Kreis in der Manafarbe mit goldenem Rand und Symbol.
    fn draw_mana(&mut self, t: ManaType, c: Vector2, r: f32) {
        let mut b = self.base_mut();
        draw_mana_symbol(&mut b, t, c, r);
    }

    /// Kleine dunkle Plakette mit der Anzahl.
    fn draw_count(&mut self, c: Vector2, n: u32) {
        let Some(font) = self.base().get_theme_default_font() else { return };
        let r = 8.0;
        self.base_mut().draw_circle(c, r, Color::from_rgba(0.05, 0.05, 0.07, 0.95));
        self.base_mut().draw_arc_ex(c, r, 0.0, TAU, 20, RIM).width(1.2).antialiased(true).done();
        let text = n.to_string();
        let w = 2.0 * r;
        self.base_mut()
            .draw_string_ex(&font, c + Vector2::new(-r, 4.0), &text)
            .alignment(HorizontalAlignment::CENTER)
            .width(w)
            .font_size(11)
            .modulate(Color::WHITE)
            .done();
    }
}

/// Mana-Symbol zeichnen (auch für die 3D-Kostensymbole, `table::cost_symbols`): das Symbol aus
/// dem Symbolbogen `Assets/Mana/Mana.svg` (`game::mana_symbols`); farblos (fehlt im Bogen) als
/// grauer Kreis mit goldenem Rand und Raute.
pub fn draw_mana_symbol(b: &mut CanvasItem, t: ManaType, c: Vector2, r: f32) {
    // Symbol aus dem Symbolbogen (`mana_symbols`, farblos aus Colorless.svg); ohne Bogen gezeichnet
    {
        let wubrg = match t {
            ManaType::White => "W",
            ManaType::Blue => "U",
            ManaType::Black => "B",
            ManaType::Red => "R",
            ManaType::Green => "G",
            ManaType::Colorless => "C",
        };
        if let Some(tex) = super::mana_symbols::texture(wubrg) {
            b.draw_circle(c + Vector2::new(0.0, 1.5), r + 1.0, Color::from_rgba(0.0, 0.0, 0.0, 0.45));
            b.draw_texture_rect(&tex, Rect2::new(c - Vector2::splat(r), Vector2::splat(2.0 * r)), false);
            return;
        }
    }
    let fill = t.color().lerp(Color::WHITE, 0.15);
    let lum = 0.299 * fill.r + 0.587 * fill.g + 0.114 * fill.b;
    let ink = if lum < 0.45 { Color::from_rgb(0.93, 0.9, 0.85) } else { Color::from_rgb(0.12, 0.1, 0.08) };
    b.draw_circle(c + Vector2::new(0.0, 1.5), r + 1.5, Color::from_rgba(0.0, 0.0, 0.0, 0.45));
    b.draw_circle(c, r, fill);
    b.draw_arc_ex(c, r, 0.0, TAU, 32, RIM).width(2.0).antialiased(true).done();
    let s = r * 0.62;
    let w = (s * 0.18).max(1.2);
    let poly = |pts: &[Vector2]| PackedVector2Array::from(pts);
    match t {
        ManaType::Green => {
            // Nadelbaum: drei gestapelte Dreiecke und ein Stamm
            b.draw_rect(Rect2::new(c + Vector2::new(-s * 0.14, s * 0.45), Vector2::new(s * 0.28, s * 0.5)), ink);
            for (top, half, bottom) in [(-0.95f32, 0.45f32, -0.2f32), (-0.6, 0.62, 0.15), (-0.25, 0.8, 0.5)] {
                b.draw_colored_polygon(
                    &poly(&[c + Vector2::new(0.0, top) * s, c + Vector2::new(half, bottom) * s, c + Vector2::new(-half, bottom) * s]),
                    ink,
                );
            }
        }
        ManaType::Blue => {
            // Wassertropfen: Spitze oben, unten rund; kleiner Glanzpunkt
            let pts: Vec<Vector2> = (0..28)
                .map(|i| {
                    let a = i as f32 / 28.0 * TAU;
                    c + Vector2::new(0.72 * a.sin() * (a * 0.5).sin(), -0.95 * a.cos()) * s + Vector2::new(0.0, s * 0.12)
                })
                .collect();
            b.draw_colored_polygon(&poly(&pts), ink);
            b.draw_circle(c + Vector2::new(-s * 0.2, s * 0.3), s * 0.12, fill);
        }
        ManaType::Black => {
            // Totenkopf: runder Schädel, Kiefer, zwei Augenhöhlen, Nasenloch, Zahnfugen
            b.draw_circle(c + Vector2::new(0.0, -s * 0.15), s * 0.72, ink);
            b.draw_rect(Rect2::new(c + Vector2::new(-s * 0.42, s * 0.2), Vector2::new(s * 0.84, s * 0.6)), ink);
            for dx in [-0.3f32, 0.3] {
                b.draw_circle(c + Vector2::new(dx * s, -s * 0.12), s * 0.2, fill);
            }
            b.draw_colored_polygon(
                &poly(&[c + Vector2::new(0.0, s * 0.12), c + Vector2::new(s * 0.1, s * 0.3), c + Vector2::new(-s * 0.1, s * 0.3)]),
                fill,
            );
            for dx in [-0.14f32, 0.14] {
                b.draw_line_ex(c + Vector2::new(dx * s, s * 0.5), c + Vector2::new(dx * s, s * 0.8), fill).width(w * 0.7).done();
            }
        }
        ManaType::White => {
            // Sonne: Scheibe mit zwölf Strahlen
            b.draw_circle(c, s * 0.45, ink);
            for k in 0..12 {
                let a = k as f32 * TAU / 12.0;
                let d = Vector2::new(a.cos(), a.sin());
                let len = if k % 2 == 0 { 0.98 } else { 0.8 };
                b.draw_line_ex(c + d * (s * 0.6), c + d * (s * len), ink).width(w).antialiased(true).done();
            }
        }
        ManaType::Red => {
            // Flamme: drei Zungen (mittlere am höchsten) auf gemeinsamem rundem Fuß
            let tongue = |x: f32, h: f32, width: f32| -> Vec<Vector2> {
                (0..20)
                    .map(|i| {
                        let a = i as f32 / 20.0 * TAU;
                        let p = Vector2::new(width * a.sin() * (a * 0.5).sin(), -h * a.cos());
                        c + Vector2::new(x, 0.35) * s + Vector2::new(p.x, if p.y < 0.0 { p.y } else { p.y * 0.45 }) * s
                    })
                    .collect()
            };
            b.draw_colored_polygon(&poly(&tongue(-0.32, 0.75, 0.4)), ink);
            b.draw_colored_polygon(&poly(&tongue(0.32, 0.75, 0.4)), ink);
            b.draw_colored_polygon(&poly(&tongue(0.0, 1.25, 0.55)), ink);
            b.draw_colored_polygon(&poly(&tongue(0.0, 0.55, 0.25)), fill);
        }
        ManaType::Colorless => {
            // Raute mit kleiner Raute innen
            let d = |k: f32| {
                poly(&[c + Vector2::new(0.0, -k) * s, c + Vector2::new(k * 0.75, 0.0) * s, c + Vector2::new(0.0, k) * s, c + Vector2::new(-k * 0.75, 0.0) * s])
            };
            b.draw_colored_polygon(&d(0.95), ink);
            b.draw_colored_polygon(&d(0.4), fill);
        }
    }
}
