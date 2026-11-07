//! PhaseTimeline: Zeitstrahl der Phasen für das HUD (unten rechts über dem Button „Nächste Phase“).
//!
//! Waagerechte Linie mit drei Stationen:
//! 1. **Rundenbeginn** – ein Kreis für Enttappen, Auffrischung, Ziehen und Hauptphase 1, der mit
//!    jedem dieser Schritte größer wird (Hauptphase 1 = großer Kreis).
//! 2. **Kampf** – ein mittlerer Punkt; seine drei Schritte (Angreifer, Blocker, Schaden) sind
//!    eingeklappt und klappen erst auf, wenn der Kampf beginnt (danach wieder zu).
//! 3. **Rundenende** – ein Kreis für Hauptphase 2 und Ende: groß in Hauptphase 2, schrumpft
//!    im Endschritt auf einen kleinen Kreis.
//!
//! Die Stationen liegen eng beieinander, mittig über dem Hauptbutton; beginnt der Kampf, klappt er
//! auf (die Reihe wächst nach beiden Seiten) und schafft Platz für seine drei Schritte, danach
//! rückt alles wieder zusammen.
//! In den Kreisen steht ein Symbol für den Schritt: Enttappen = Kreispfeil gegen den
//! Uhrzeigersinn, Auffrischung = Sonne, Ziehen = Karte mit Plus, Hauptphase = aufgefächerte
//! Karten, Ende = Mondsichel, Kampf = gekreuzte Schwerter; aufgeklappte Kampfschritte:
//! Angreifer = Schwert, Blocker = Schild, Schaden = Flamme.
//!
//! Die aktuelle Station leuchtet (weiß mit goldenem Rand, pulsierender Schein); erledigte Strecke
//! und Stationen sind golden, kommende hohl. Größe, Aufklappen und Position gleiten weich.
//! Über der Linie steht der Name der aktuellen Phase.

use std::f32::consts::TAU;
use std::ops::RangeInclusive;

use godot::classes::control::{LayoutPreset, MouseFilter};
use godot::classes::{Control, IControl};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

const GOLD: Color = Color::from_rgb(0.95, 0.78, 0.4);
const DIM: Color = Color::from_rgba(0.93, 0.87, 0.74, 0.55);
const LINE: Color = Color::from_rgba(1.0, 1.0, 1.0, 0.22);
const HOLLOW: Color = Color::from_rgba(0.05, 0.06, 0.08, 0.9);

/// Phasen-Indizes des Rundenbeginns (Enttappen, Auffrischung, Ziehen, Hauptphase 1) – ein wachsender Kreis.
const OPENING: RangeInclusive<usize> = 0..=3;
/// Phasen-Indizes der Kampfschritte (Angreifer, Blocker, Schaden) – eingeklappte Unterpunkte.
const COMBAT: RangeInclusive<usize> = 4..=6;
/// Phasen-Indizes des Rundenendes (Hauptphase 2, Ende) – ein schrumpfender Kreis.
const CLOSING: RangeInclusive<usize> = 7..=8;
const END: usize = 8;

const WIDTH: f32 = 270.0;
const HEIGHT: f32 = 58.0;
const LINE_Y: f32 = 40.0;
/// Radien: Rundenbeginn wächst von OPENING_R[0] bis BIG_R; Kampf mittel, Unterpunkte klein;
/// Rundenende groß (BIG_R) und im Endschritt klein (END_R).
const OPENING_R: [f32; 4] = [9.0, 10.5, 12.0, BIG_R];
const BIG_R: f32 = 14.0;
const COMBAT_R: f32 = 9.0;
const SUB_R: f32 = 8.0;
const END_R: f32 = 9.5;
/// Abstand zwischen zwei Stationen (Pixel) und zwischen aufgeklappten Kampfschritten.
const STATION_GAP: f32 = 54.0;
const SUB_GAP: f32 = 29.0;
/// Dunkle Symbolfarbe auf hellen/goldenen Kreisen.
const ICON_DARK: Color = Color::from_rgb(0.16, 0.12, 0.08);

/// Zeitstrahl der Phasen (HUD, unten rechts über dem Hauptbutton): ein mit Enttappen, Auffrischung,
/// Ziehen und Hauptphase 1 wachsender Kreis, ein Punkt „Kampf“, der seine drei Schritte (Phasen-Index
/// 4–6) erst während des Kampfs aufklappt, und ein Kreis für Hauptphase 2 und Ende, der im
/// Endschritt kleiner wird. Die aktuelle Station leuchtet; erledigte sind golden, kommende hohl. Darüber steht der Name
/// der aktuellen Phase. Wird vom [TurnHud] jedes Frame über
/// [method set_state] gefüttert.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct PhaseTimeline {
    titles: Vec<String>,
    current: i64,
    /// Animierte Werte (None = noch nicht gesetzt): x der Markierung, Radius des
    /// Rundenbeginn- und Rundenende-Kreises, Aufklappgrad des Kampfs (0 = zu, 1 = offen).
    marker_x: Option<f32>,
    opening_r: Option<f32>,
    closing_r: Option<f32>,
    expand: f32,
    time: f32,
    /// Zählt Änderungen der Phasennamen (für [`Look`]).
    titles_gen: u32,
    /// Eigene Ebene für die aktuelle Station mit ihrem pulsierenden Schein: Nur sie wird jedes
    /// Bild neu gezeichnet, der übrige Zeitstrahl nur, wenn sich etwas daran ändert.
    pulse_layer: Option<Gd<Control>>,
    /// Stand, mit dem der Zeitstrahl zuletzt gezeichnet wurde.
    drawn: Option<Look>,
    base: Base<Control>,
}

/// Alles, wovon der ruhende Zeitstrahl abhängt (aktuelle Phase, gleitende Werte, Namen).
type Look = (i64, Option<f32>, Option<f32>, Option<f32>, f32, u32);

/// Zustand einer Station relativ zur aktuellen Phase.
#[derive(Clone, Copy, PartialEq)]
enum State {
    Done,
    Current,
    Upcoming,
}

/// Symbol in einem Kreis des Zeitstrahls.
#[derive(Clone, Copy, PartialEq)]
enum Icon {
    Untap,
    Upkeep,
    Draw,
    Main,
    End,
    Combat,
    Attack,
    Block,
    Damage,
}

/// Ein Punkt auf dem Zeitstrahl.
struct Dot {
    x: f32,
    r: f32,
    alpha: f32,
    state: State,
    icon: Option<Icon>,
}

#[godot_api]
impl IControl for PhaseTimeline {
    fn init(base: Base<Control>) -> Self {
        Self {
            titles: Vec::new(),
            current: -1,
            marker_x: None,
            opening_r: None,
            closing_r: None,
            expand: 0.0,
            time: 0.0,
            titles_gen: 0,
            pulse_layer: None,
            drawn: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.base_mut().set_custom_minimum_size(Vector2::new(WIDTH, HEIGHT));
        self.base_mut().set_mouse_filter(MouseFilter::IGNORE);
        let mut layer = Control::new_alloc();
        layer.set_name("Pulse");
        layer.set_anchors_preset(LayoutPreset::FULL_RECT);
        layer.set_mouse_filter(MouseFilter::IGNORE);
        layer.connect("draw", &Callable::from_object_method(&self.to_gd(), "draw_pulse"));
        self.base_mut().add_child(&layer);
        self.pulse_layer = Some(layer);
    }

    fn process(&mut self, delta: f64) {
        let k = 1.0 - (-8.0 * delta as f32).exp();
        self.time += delta as f32;
        // Neue Runde: Größe springt zurück statt zu schrumpfen
        let target_r = self.opening_target_r();
        self.opening_r = Some(match self.opening_r {
            Some(r) if target_r >= r - 0.1 => r + (target_r - r) * k,
            _ => target_r,
        });
        // Rundenende: schrumpft im Endschritt, springt bei neuer Runde zurück auf groß
        let target_c = self.closing_target_r();
        self.closing_r = Some(match self.closing_r {
            Some(r) if target_c <= r + 0.1 => r + (target_c - r) * k,
            _ => target_c,
        });
        let target_e = if self.combat_step().is_some() { 1.0 } else { 0.0 };
        self.expand += (target_e - self.expand) * k;

        let target = self.target_x(&self.dots());
        self.marker_x = Some(match self.marker_x {
            // Neue Runde (zurück zum Anfang): springen statt rückwärts gleiten
            Some(x) if target >= x - 0.5 => x + (target - x) * k,
            _ => target,
        });
        // am Ziel einrasten, damit der ruhende Zeitstrahl bald nicht mehr neu gezeichnet wird
        let snap = |v: f32, goal: f32| if (goal - v).abs() < 0.01 { goal } else { v };
        self.opening_r = self.opening_r.map(|r| snap(r, target_r));
        self.closing_r = self.closing_r.map(|r| snap(r, target_c));
        self.expand = snap(self.expand, target_e);
        self.marker_x = self.marker_x.map(|x| snap(x, target));
        if self.drawn != Some(self.look()) {
            self.base_mut().queue_redraw();
        }
        if self.current >= 0 {
            if let Some(l) = self.pulse_layer.as_mut() {
                l.queue_redraw();
            }
        }
    }

    /// Ruhender Zeitstrahl ohne die aktuelle Station (die zeichnet `draw_pulse`).
    fn draw(&mut self) {
        self.drawn = Some(self.look());
        let dots = self.dots();
        let marker = self.marker_x.unwrap_or(dots[0].x);
        let (x0, x1) = (dots[0].x, dots[dots.len() - 1].x);

        // Linie: gesamt gedimmt, erledigte Strecke golden
        self.line(x0, x1, LINE, 2.0);
        if self.current >= 0 {
            self.line(x0, marker, GOLD, 3.0);
        }

        for d in dots.iter().filter(|d| d.state != State::Current) {
            let mut me = self.base_mut();
            draw_dot(&mut me, d, 0.0);
        }

        // Name der aktuellen Phase über der Linie (im Rahmen gehalten)
        let title = self.current_title();
        // Über der Markierung, aber anhand der echten Textbreite im Rahmen gehalten
        let half = self.base().get_theme_default_font().map_or(100.0, |f| {
            f.get_string_size_ex(&title).font_size(16).done().x * 0.5 + 4.0
        });
        let tx = marker.clamp(half.min(WIDTH * 0.5), (WIDTH - half).max(WIDTH * 0.5));
        self.text(&title, Vector2::new(tx, LINE_Y - BIG_R - 8.0), 16, GOLD, HorizontalAlignment::CENTER);
    }
}

#[godot_api]
impl PhaseTimeline {
    /// Setzt Phasennamen (in Spielreihenfolge) und Index der aktuellen Phase (-1 = vor Spielbeginn).
    #[func]
    pub fn set_state(&mut self, titles: PackedStringArray, current: i64) {
        if titles.len() != self.titles.len() || titles.as_slice().iter().zip(&self.titles).any(|(a, b)| *a != b.as_str()) {
            self.titles = titles.as_slice().iter().map(|t| t.to_string()).collect();
            self.titles_gen = self.titles_gen.wrapping_add(1);
        }
        self.current = current;
    }

    /// Zeichnet die aktuelle Station mit pulsierendem Schein in die eigene Ebene (jedes Bild;
    /// gerufen vom Signal `draw` der Ebene).
    #[func]
    fn draw_pulse(&mut self) {
        let Some(mut layer) = self.pulse_layer.clone() else { return };
        let pulse = 0.5 + 0.5 * (self.time * 3.0).sin();
        for d in self.dots().iter().filter(|d| d.state == State::Current) {
            draw_dot(&mut layer, d, pulse);
        }
    }
}

/// Zeichnet eine Station (Kreis, Rand, Symbol) auf `canvas`; `pulse` (0–1) = Stärke des Scheins
/// der aktuellen Station.
fn draw_dot(canvas: &mut Gd<Control>, d: &Dot, pulse: f32) {
    if d.alpha < 0.02 {
        return;
    }
    let c = Vector2::new(d.x, LINE_Y);
    let fade = |col: Color| Color::from_rgba(col.r, col.g, col.b, col.a * d.alpha);
    let (fill, ink) = match d.state {
        State::Done => {
            canvas.draw_circle(c, d.r, fade(GOLD));
            (GOLD, ICON_DARK)
        }
        State::Upcoming => {
            canvas.draw_circle(c, d.r, fade(HOLLOW));
            canvas.draw_arc_ex(c, d.r, 0.0, TAU, 28, fade(DIM)).width(1.5).antialiased(true).done();
            (HOLLOW, DIM)
        }
        State::Current => {
            let glow = Color::from_rgba(GOLD.r, GOLD.g, GOLD.b, (0.18 + 0.15 * pulse) * d.alpha);
            canvas.draw_circle(c, d.r + 4.0 + 2.0 * pulse, glow);
            canvas.draw_circle(c, d.r, fade(Color::WHITE));
            canvas.draw_arc_ex(c, d.r, 0.0, TAU, 28, fade(GOLD)).width(2.5).antialiased(true).done();
            (Color::WHITE, ICON_DARK)
        }
    };
    if let Some(icon) = d.icon {
        draw_icon(canvas, icon, c, d.r * 0.62, fade(ink), fill);
    }
}

impl PhaseTimeline {
    fn look(&self) -> Look {
        (self.current, self.marker_x, self.opening_r, self.closing_r, self.expand, self.titles_gen)
    }

    fn current_index(&self) -> Option<usize> {
        usize::try_from(self.current).ok()
    }

    /// Index des aktuellen Kampfschritts (0–2) oder None außerhalb des Kampfs.
    fn combat_step(&self) -> Option<usize> {
        self.current_index().filter(|i| COMBAT.contains(i)).map(|i| i - COMBAT.start())
    }

    /// Zielradius des Rundenbeginn-Kreises: wächst mit Enttappen → Hauptphase 1, danach groß.
    fn opening_target_r(&self) -> f32 {
        match self.current_index() {
            None => OPENING_R[0],
            Some(i) if OPENING.contains(&i) => OPENING_R[i],
            Some(_) => BIG_R,
        }
    }

    /// Zielradius des Rundenende-Kreises: groß, im Endschritt klein.
    fn closing_target_r(&self) -> f32 {
        if self.current_index() == Some(END) { END_R } else { BIG_R }
    }

    /// Zustand einer Station, die die Phasen `range` umfasst.
    fn state(&self, range: RangeInclusive<usize>) -> State {
        match self.current_index() {
            None => State::Upcoming,
            Some(i) if range.contains(&i) => State::Current,
            Some(i) if i > *range.end() => State::Done,
            Some(_) => State::Upcoming,
        }
    }

    /// Stationen mit (animierter) Position und Größe, mittig: Abstand [STATION_GAP]; beim
    /// Aufklappen des Kampfs kommen 3 × [SUB_GAP] dazu und die Reihe wächst nach beiden Seiten.
    fn dots(&self) -> Vec<Dot> {
        let e = self.expand;
        let spread = 3.0 * SUB_GAP * e;
        let total = 2.0 * STATION_GAP + spread;
        // Mittig im Rahmen (über dem Hauptbutton); Aufklappen wächst nach beiden Seiten
        let start = (WIDTH - total) * 0.5;
        let x = |u: f32| start + u;
        let opening_r = self.opening_r.unwrap_or_else(|| self.opening_target_r());
        let cur = self.current_index();
        let opening_icon = match cur {
            None | Some(0) => Icon::Untap,
            Some(1) => Icon::Upkeep,
            Some(2) => Icon::Draw,
            Some(_) => Icon::Main,
        };
        let closing_icon = if cur == Some(END) { Icon::End } else { Icon::Main };

        let mut dots = vec![Dot { x: x(0.0), r: opening_r, alpha: 1.0, state: self.state(OPENING), icon: Some(opening_icon) }];
        // Kampf: Gruppenpunkt; ist er aufgeklappt, übernimmt der aktuelle Schritt die Hervorhebung
        let mut combat_state = self.state(COMBAT);
        if combat_state == State::Current && e > 0.5 {
            combat_state = State::Done;
        }
        dots.push(Dot { x: x(STATION_GAP), r: COMBAT_R, alpha: 1.0, state: combat_state, icon: Some(Icon::Combat) });
        let step = self.combat_step();
        for k in 0..3 {
            let state = match step {
                Some(s) if s == k => State::Current,
                Some(s) if s > k => State::Done,
                _ => State::Upcoming,
            };
            let u = STATION_GAP + (k as f32 + 1.0) * SUB_GAP * e;
            let icon = [Icon::Attack, Icon::Block, Icon::Damage][k];
            dots.push(Dot { x: x(u), r: SUB_R, alpha: e, state, icon: Some(icon) });
        }
        let closing_r = self.closing_r.unwrap_or_else(|| self.closing_target_r());
        dots.push(Dot { x: x(2.0 * STATION_GAP + spread), r: closing_r, alpha: 1.0, state: self.state(CLOSING), icon: Some(closing_icon) });
        dots
    }

    /// x-Position der aktuellen Station (vor Spielbeginn: erste Station; nach dem Ende: letzte).
    fn target_x(&self, dots: &[Dot]) -> f32 {
        let current = dots.iter().filter(|d| d.state == State::Current).max_by(|a, b| a.alpha.total_cmp(&b.alpha));
        match (current, self.current_index()) {
            (Some(d), _) => d.x,
            (None, Some(_)) => dots[dots.len() - 1].x,
            (None, None) => dots[0].x,
        }
    }

    fn current_title(&self) -> String {
        let Some(i) = self.current_index() else {
            return tr!("Austeilen …", "Dealing …").to_string();
        };
        let title = self.titles.get(i).cloned().unwrap_or_default();
        if COMBAT.contains(&i) {
            trf!("Kampf · {title}", "Combat · {title}")
        } else {
            title
        }
    }

    fn line(&mut self, from: f32, to: f32, color: Color, width: f32) {
        if to <= from {
            return;
        }
        self.base_mut()
            .draw_line_ex(Vector2::new(from, LINE_Y), Vector2::new(to, LINE_Y), color)
            .width(width)
            .antialiased(true)
            .done();
    }

    /// Text mit dunkler Kontur; `pos` = Grundlinie, bei CENTER mittig.
    fn text(&mut self, text: &str, pos: Vector2, size: i32, color: Color, align: HorizontalAlignment) {
        let Some(font) = self.base().get_theme_default_font() else { return };
        let width = WIDTH;
        let p = pos - Vector2::new(width * 0.5, 0.0);
        self.base_mut()
            .draw_string_outline_ex(&font, p, text)
            .alignment(align)
            .width(width)
            .font_size(size)
            .size(5)
            .modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.85))
            .done();
        self.base_mut()
            .draw_string_ex(&font, p, text)
            .alignment(align)
            .width(width)
            .font_size(size)
            .modulate(color)
            .done();
    }
}

/// Zeichnet ein Schritt-Symbol mittig bei `c` (Größe `s` ≈ halbe Symbolbreite) in `ink`;
/// `bg` = Füllfarbe des Kreises (für die Aussparung der Mondsichel).
fn draw_icon(b: &mut Gd<Control>, icon: Icon, c: Vector2, s: f32, ink: Color, bg: Color) {
    let w = (s * 0.28).max(1.2);
    match icon {
        Icon::Untap => {
            // Kreispfeil gegen den Uhrzeigersinn (Lücke oben rechts, Spitze zeigt nach oben)
            let r = s * 0.78;
            let (a0, a1) = (-0.35 * std::f32::consts::PI, 1.25 * std::f32::consts::PI);
            b.draw_arc_ex(c, r, a0, a1, 20, ink).width(w).antialiased(true).done();
            let tip = c + Vector2::new(a0.cos(), a0.sin()) * r;
            let along = Vector2::new(a0.sin(), -a0.cos()); // Laufrichtung am Bogenanfang (gegen den Uhrzeigersinn)
            let side = Vector2::new(a0.cos(), a0.sin());
            let head = [tip + along * (s * 0.55), tip + side * (s * 0.42), tip - side * (s * 0.42)];
            b.draw_colored_polygon(&PackedVector2Array::from(&head[..]), ink);
        }
        Icon::Upkeep => {
            // Sonne: Scheibe mit acht Strahlen
            b.draw_circle(c, s * 0.42, ink);
            for k in 0..8 {
                let a = k as f32 * TAU / 8.0;
                let d = Vector2::new(a.cos(), a.sin());
                b.draw_line_ex(c + d * (s * 0.62), c + d * (s * 0.95), ink).width(w * 0.8).antialiased(true).done();
            }
        }
        Icon::Draw => {
            // Karte (Hochformat) mit kleinem Plus
            let size = Vector2::new(s * 1.05, s * 1.45);
            let rect = Rect2::new(c - size * 0.5 - Vector2::new(s * 0.18, 0.0), size);
            b.draw_rect_ex(rect, ink).filled(false).width(w).done();
            let p = c + Vector2::new(s * 0.62, -s * 0.45);
            b.draw_line_ex(p - Vector2::new(s * 0.3, 0.0), p + Vector2::new(s * 0.3, 0.0), ink).width(w).done();
            b.draw_line_ex(p - Vector2::new(0.0, s * 0.3), p + Vector2::new(0.0, s * 0.3), ink).width(w).done();
        }
        Icon::Main => {
            // Drei aufgefächerte Karten
            for k in [-1.0f32, 0.0, 1.0] {
                let a = k * 0.38;
                let (sin, cos) = a.sin_cos();
                let rot = |p: Vector2| c + Vector2::new(0.0, s * 0.35) + Vector2::new(p.x * cos - p.y * sin, p.x * sin + p.y * cos);
                let (hw, h) = (s * 0.36, s * 1.2);
                let quad = [rot(Vector2::new(-hw, -h)), rot(Vector2::new(hw, -h)), rot(Vector2::new(hw, 0.0)), rot(Vector2::new(-hw, 0.0))];
                b.draw_colored_polygon(&PackedVector2Array::from(&quad[..]), bg);
                let outline = [quad[0], quad[1], quad[2], quad[3], quad[0]];
                b.draw_polyline_ex(&PackedVector2Array::from(&outline[..]), ink).width(w * 0.8).antialiased(true).done();
            }
        }
        Icon::End => {
            // Mondsichel: Scheibe, davon eine versetzte Scheibe in Hintergrundfarbe ausgespart
            b.draw_circle(c, s * 0.8, ink);
            b.draw_circle(c + Vector2::new(s * 0.38, -s * 0.22), s * 0.68, bg);
        }
        Icon::Attack => {
            // Einzelnes Schwert, schräg (Spitze oben rechts)
            let tip = c + Vector2::new(s * 0.85, -s * 0.85);
            let hilt = c + Vector2::new(-s * 0.8, s * 0.8);
            b.draw_line_ex(hilt, tip, ink).width(w).antialiased(true).done();
            let g = c + Vector2::new(-s * 0.4, s * 0.4);
            let n = Vector2::new(1.0, 1.0).normalized() * (s * 0.38);
            b.draw_line_ex(g - n, g + n, ink).width(w).antialiased(true).done();
        }
        Icon::Block => {
            // Schild: oben gerade Kante, Seiten nach unten zur Spitze zulaufend
            let pts: Vec<Vector2> = [
                (-0.8, -0.8), (0.8, -0.8), (0.78, -0.1), (0.55, 0.45), (0.0, 0.95), (-0.55, 0.45), (-0.78, -0.1),
            ]
            .iter()
            .map(|(x, y)| c + Vector2::new(*x, *y) * s)
            .collect();
            b.draw_colored_polygon(&PackedVector2Array::from(&pts[..]), ink);
            // Mittelstreifen in Hintergrundfarbe
            b.draw_line_ex(c + Vector2::new(0.0, -0.6) * s, c + Vector2::new(0.0, 0.62) * s, bg).width(w * 0.7).done();
        }
        Icon::Damage => {
            // Flamme: Tropfenform mit Spitze nach oben, innen eine kleinere Flamme ausgespart
            // Tropfenkurve: x = sin t · sin(t/2), y = −cos t → Spitze oben, unten rund
            let flame = |scale: f32, off: Vector2| -> Vec<Vector2> {
                (0..24)
                    .map(|i| {
                        let t = i as f32 / 24.0 * TAU;
                        let p = Vector2::new(0.78 * t.sin() * (t * 0.5).sin(), -0.95 * t.cos());
                        c + off + p * (s * scale)
                    })
                    .collect()
            };
            b.draw_colored_polygon(&PackedVector2Array::from(&flame(1.0, Vector2::ZERO)[..]), ink);
            b.draw_colored_polygon(&PackedVector2Array::from(&flame(0.45, Vector2::new(0.0, s * 0.25))[..]), bg);
        }
        Icon::Combat => {
            // Zwei gekreuzte Schwerter (Klinge + Parierstange)
            for dir in [1.0f32, -1.0] {
                let tip = c + Vector2::new(dir * s * 0.8, -s * 0.8);
                let hilt = c + Vector2::new(-dir * s * 0.75, s * 0.75);
                b.draw_line_ex(hilt, tip, ink).width(w).antialiased(true).done();
                let g = c + Vector2::new(-dir * s * 0.4, s * 0.4);
                let n = Vector2::new(dir, dir).normalized() * (s * 0.3);
                b.draw_line_ex(g - n, g + n, ink).width(w).antialiased(true).done();
            }
        }
    }
}
