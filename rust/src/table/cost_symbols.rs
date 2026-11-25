//! Kostensymbole in 3D: der gemeinsame Baustein für jede Kostenanzeige der 3D-Oberfläche
//! (Kosten über den Handkarten wie im Video „Spree und Stack“, Knöpfe wie „Bezahlen {1}{U}“,
//! Beschriftungen wie „+ {3}“).
//!
//! Die Symbole stammen aus dem Symbolbogen `Assets/Mana/Mana.svg` (`game::mana_symbols`):
//! Farben, Zahlen, X, Hybride, phyrexianische Symbole, Tappen. Was dort fehlt (farblos {C},
//! Hybrid-phyrexianisch), wird gezeichnet: farblos als graue Raute ([`draw_mana_symbol`]),
//! anderes als Text in einem grauen Kreis. Jedes Symbol wird einmal in eine kleine `SubViewport` gezeichnet
//! und als Textur zwischengespeichert; in der Szene ist es ein `Sprite3D`.

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::base_material_3d::{BillboardMode, TextureFilter};
use godot::classes::control::MouseFilter;
use godot::classes::label_3d::DrawFlags as LabelFlags;
use godot::classes::sprite_base_3d::DrawFlags as SpriteFlags;
use godot::classes::sub_viewport::UpdateMode;
use godot::classes::{
    Control, Engine, IControl, Label3D, RichTextLabel, SceneTree, Sprite3D, SubViewport, Texture2D, ThemeDb,
};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use crate::game::mana::ManaType;
use crate::game::mana_view::draw_mana_symbol;

/// Kantenlänge einer Symbol-Textur (Pixel).
const TEX: i32 = 128;
/// Abstand zwischen zwei Symbolen einer Reihe (Anteil der Symbolhöhe).
const SYMBOL_GAP: f32 = 0.08;
/// Abstand zwischen Text und Symbolen (Anteil der Symbolhöhe).
const TEXT_GAP: f32 = 0.22;
/// Höhe eines Symbols im Verhältnis zur Schriftgröße einer Beschriftung.
const SYMBOL_PER_FONT: f32 = 0.82;

/// Zerlegt Kosten in Symbole: „{2}{W}{W}“, „2 W W“, „X U“ → `["2", "W", "W"]`; Hybrid und
/// phyrexianisch bleiben ein Symbol („W/U“, „R/P“).
pub fn cost_tokens(text: &str) -> Vec<String> {
    text.split(|c: char| c.is_whitespace() || c == '{' || c == '}')
        .filter(|t| !t.is_empty())
        .map(|t| t.to_uppercase())
        .collect()
}

/// Zeichenfläche eines einzelnen Kostensymbols (intern: liegt in einer unsichtbaren `SubViewport`, deren Textur die 3D-Kostensymbole nutzen).
#[derive(GodotClass)]
#[class(base=Control, init)]
pub struct CostSymbolCanvas {
    token: String,
    base: Base<Control>,
}

#[godot_api]
impl IControl for CostSymbolCanvas {
    fn draw(&mut self) {
        let size = TEX as f32;
        let c = Vector2::splat(size * 0.5);
        let r = size * 0.44;
        let token = self.token.clone();
        let mana = (token.len() == 1).then(|| token.chars().next().and_then(|ch| if ch == 'C' { Some(ManaType::Colorless) } else { ManaType::from_wubrg(ch) })).flatten();
        let mut b = self.base_mut();
        if let Some(t) = mana {
            draw_mana_symbol(&mut b, t, c, r);
            return;
        }
        // Kennzeichen eines Spielers (Monarch, Initiative, Ring, Gift …): „§Art:Anzahl“
        if let Some(rest) = token.strip_prefix('§') {
            let (kind, count) = rest.split_once(':').unwrap_or((rest, ""));
            draw_seat_token(&mut b, kind, count.parse().unwrap_or(0), size);
            return;
        }
        // Schlüsselwort-Symbol unten an der Kreaturen-Kachel („&flying“ …, [`keyword_icon`])
        if let Some(kind) = token.strip_prefix('&') {
            draw_keyword(&mut b, kind, size);
            return;
        }
        // Loyalität (CR 306.5, 606): Schild bzw. Kosten-Symbol einer Loyalitätsfähigkeit
        if let Some(rest) = token.strip_prefix('%') {
            let (kind, value) = rest.split_once(':').unwrap_or((rest, ""));
            draw_loyalty(&mut b, kind, value, size);
            return;
        }
        // Marken-Münze („#Rand:Anzahl“, siehe [`counter_coin`]): silbern mit farbigem Rand
        if let Some(rest) = token.strip_prefix('#') {
            let (rim, count) = rest.split_once(':').unwrap_or(("gold", rest));
            let rim = rim.to_lowercase();
            let rim_color = match rim.as_str() {
                "green" => Color::from_rgb(0.35, 0.85, 0.4),
                "red" => Color::from_rgb(0.95, 0.35, 0.3),
                "blue" => Color::from_rgb(0.35, 0.6, 1.0),
                _ => Color::from_rgb(0.9, 0.75, 0.4),
            };
            b.draw_circle(c + Vector2::new(0.0, size * 0.03), r + size * 0.02, Color::from_rgba(0.0, 0.0, 0.0, 0.5));
            b.draw_circle(c, r, Color::from_rgb(0.62, 0.64, 0.7));
            // Glanz: hellere Scheibe oben links, dunklerer Ring innen
            b.draw_circle(c - Vector2::splat(size * 0.04), r * 0.82, Color::from_rgb(0.86, 0.88, 0.93));
            b.draw_arc_ex(c, r * 0.8, 0.0, std::f32::consts::TAU, 48, Color::from_rgb(0.5, 0.52, 0.58)).width(size * 0.025).antialiased(true).done();
            b.draw_arc_ex(c, r, 0.0, std::f32::consts::TAU, 48, rim_color).width(size * 0.06).antialiased(true).done();
            let Some(font) = ThemeDb::singleton().get_fallback_font() else { return };
            let font_size = if count.chars().count() <= 1 { 80 } else if count.chars().count() == 2 { 64 } else { 46 };
            let ascent = font.get_ascent_ex().font_size(font_size).done();
            let descent = font.get_descent_ex().font_size(font_size).done();
            b.draw_string_ex(&font, Vector2::new(0.0, c.y + (ascent - descent) * 0.5), count)
                .alignment(HorizontalAlignment::CENTER)
                .width(size)
                .font_size(font_size)
                .modulate(Color::from_rgb(0.12, 0.12, 0.16))
                .done();
            return;
        }
        // Zahl, X oder sonstiges Symbol: grauer Kreis mit goldenem Rand und dunkler Schrift
        b.draw_circle(c + Vector2::new(0.0, size * 0.02), r + size * 0.02, Color::from_rgba(0.0, 0.0, 0.0, 0.45));
        b.draw_circle(c, r, Color::from_rgb(0.78, 0.76, 0.72));
        b.draw_arc_ex(c, r, 0.0, std::f32::consts::TAU, 48, Color::from_rgb(0.9, 0.72, 0.36)).width(size * 0.035).antialiased(true).done();
        let Some(font) = ThemeDb::singleton().get_fallback_font() else { return };
        let font_size = if token.chars().count() <= 1 { 84 } else if token.chars().count() == 2 { 66 } else { 44 };
        let ascent = font.get_ascent_ex().font_size(font_size).done();
        let descent = font.get_descent_ex().font_size(font_size).done();
        let baseline = c.y + (ascent - descent) * 0.5;
        b.draw_string_ex(&font, Vector2::new(0.0, baseline), &token)
            .alignment(HorizontalAlignment::CENTER)
            .width(size)
            .font_size(font_size)
            .modulate(Color::from_rgb(0.1, 0.09, 0.08))
            .done();
    }
}

/// Loyalitäts-Symbol zeichnen (wie in Arena): `shield` = Loyalitäts-Schild des Planeswalkers
/// (schwarz, weißer Rand, weiße Zahl, Spitze unten); `up` = Kosten „+N“ (heller Schild mit Spitze
/// nach oben), `down` = „−N“ (dunkler Schild mit Spitze nach unten), `zero` = „0“ (grauer Schild).
fn draw_loyalty(b: &mut godot::classes::CanvasItem, kind: &str, value: &str, size: f32) {
    let pt = |x: f32, y: f32| Vector2::new(x * size, y * size);
    let shape: Vec<Vector2> = match kind {
        "up" => vec![pt(0.5, 0.05), pt(0.9, 0.3), pt(0.9, 0.84), pt(0.1, 0.84), pt(0.1, 0.3)],
        "down" => vec![pt(0.1, 0.16), pt(0.9, 0.16), pt(0.9, 0.7), pt(0.5, 0.95), pt(0.1, 0.7)],
        "zero" => vec![pt(0.1, 0.18), pt(0.9, 0.18), pt(0.9, 0.82), pt(0.1, 0.82)],
        // Schild: oben flach mit abgeschrägten Ecken, unten spitz
        _ => vec![pt(0.2, 0.06), pt(0.8, 0.06), pt(0.92, 0.2), pt(0.88, 0.6), pt(0.5, 0.96), pt(0.12, 0.6), pt(0.08, 0.2)],
    };
    let (fill, rim, ink) = match kind {
        "up" => (Color::from_rgb(0.93, 0.92, 0.88), Color::from_rgb(0.15, 0.15, 0.17), Color::from_rgb(0.08, 0.08, 0.1)),
        "zero" => (Color::from_rgb(0.55, 0.55, 0.58), Color::from_rgb(0.15, 0.15, 0.17), Color::from_rgb(0.97, 0.97, 0.97)),
        _ => (Color::from_rgb(0.07, 0.07, 0.08), Color::from_rgb(0.95, 0.95, 0.95), Color::from_rgb(0.98, 0.98, 0.98)),
    };
    let center = shape.iter().fold(Vector2::ZERO, |a, p| a + *p) / shape.len() as f32;
    let grow = |k: f32| -> PackedVector2Array { shape.iter().map(|p| center + (*p - center) * k).collect() };
    // Schatten, Rand, Füllung
    let shadow: PackedVector2Array = grow(1.04).as_slice().iter().map(|p| *p + Vector2::new(0.0, size * 0.025)).collect();
    b.draw_colored_polygon(&shadow, Color::from_rgba(0.0, 0.0, 0.0, 0.5));
    b.draw_colored_polygon(&grow(1.04), rim);
    b.draw_colored_polygon(&grow(0.9), fill);
    let Some(font) = ThemeDb::singleton().get_fallback_font() else { return };
    let n = value.chars().count();
    let font_size = if n <= 1 { 72 } else if n == 2 { 62 } else { 46 };
    let ascent = font.get_ascent_ex().font_size(font_size).done();
    let descent = font.get_descent_ex().font_size(font_size).done();
    // Mitte der Schrift: beim Schild etwas über der Spitze
    let cy = match kind {
        "shield" | "" => size * 0.46,
        "up" => size * 0.6,
        "down" => size * 0.46,
        _ => size * 0.5,
    };
    b.draw_string_ex(&font, Vector2::new(0.0, cy + (ascent - descent) * 0.5), value)
        .alignment(HorizontalAlignment::CENTER)
        .width(size)
        .font_size(font_size)
        .modulate(ink)
        .done();
}

/// Schlüsselwort-Symbol zeichnen (eigene Zeichnung, nicht aus Arena): dunkle Scheibe mit hellem
/// Rand, darauf eine weiße Figur je Schlüsselwort (`kind` = [`KeywordIcon::id`]): Flügel (Flugfähigkeit),
/// Schwert (Erstschlag), zwei gekreuzte Schwerter (Doppelschlag), Totenkopf (Todesberührung),
/// gehörnter Kopf (Bedrohlich), Pfeil (Trampelschaden), Auge (Wachsamkeit), Herz
/// (Lebensverknüpfung), Pfeil mit Bogen (Reichweite), Blitz (Eile), Schild (Fluchsicher), Schild
/// mit Punkt (Abwehr), Diamant (Unzerstörbar), Mauer (Verteidiger).
///
/// [`KeywordIcon::id`]: crate::game::cards::KeywordIcon::id
fn draw_keyword(b: &mut godot::classes::CanvasItem, kind: &str, size: f32) {
    use std::f32::consts::{PI, TAU};
    let pt = |x: f32, y: f32| Vector2::new(x * size, y * size);
    let poly = |v: &[(f32, f32)]| -> PackedVector2Array { v.iter().map(|&(x, y)| pt(x, y)).collect() };
    let ink = Color::from_rgb(0.97, 0.97, 0.96);
    let dark = Color::from_rgb(0.09, 0.09, 0.11);
    let c = pt(0.5, 0.5);
    let r = size * 0.44;
    // Scheibe: Schatten, heller Rand, dunkle Füllung
    b.draw_circle(c + Vector2::new(0.0, size * 0.03), r + size * 0.02, Color::from_rgba(0.0, 0.0, 0.0, 0.5));
    b.draw_circle(c, r, Color::from_rgb(0.88, 0.86, 0.8));
    b.draw_circle(c, r - size * 0.05, dark);
    // Schwert (senkrecht, Spitze oben) um die Mitte gedreht um `angle`
    let sword = |b: &mut godot::classes::CanvasItem, angle: f32, scale: f32| {
        let (sn, cs) = angle.sin_cos();
        let tf = |x: f32, y: f32| {
            let (dx, dy) = ((x - 0.5) * scale, (y - 0.5) * scale);
            pt(0.5 + dx * cs - dy * sn, 0.5 + dx * sn + dy * cs)
        };
        let shape = |v: &[(f32, f32)]| -> PackedVector2Array { v.iter().map(|&(x, y)| tf(x, y)).collect() };
        b.draw_colored_polygon(&shape(&[(0.5, 0.14), (0.565, 0.25), (0.565, 0.62), (0.435, 0.62), (0.435, 0.25)]), ink);
        b.draw_colored_polygon(&shape(&[(0.33, 0.62), (0.67, 0.62), (0.67, 0.68), (0.33, 0.68)]), ink);
        b.draw_colored_polygon(&shape(&[(0.47, 0.68), (0.53, 0.68), (0.53, 0.8), (0.47, 0.8)]), ink);
        b.draw_circle(tf(0.5, 0.835), size * 0.045 * scale, ink);
    };
    match kind {
        "flying" => {
            b.draw_colored_polygon(
                &poly(&[
                    (0.2, 0.66), (0.26, 0.44), (0.4, 0.3), (0.62, 0.21), (0.82, 0.2), (0.7, 0.3),
                    (0.82, 0.32), (0.66, 0.42), (0.76, 0.45), (0.58, 0.54), (0.64, 0.58), (0.42, 0.64),
                ]),
                ink,
            );
        }
        "first_strike" => sword(b, 0.0, 1.0),
        "double_strike" => {
            sword(b, -0.5, 0.92);
            sword(b, 0.5, 0.92);
        }
        "deathtouch" => {
            b.draw_circle(pt(0.5, 0.44), size * 0.22, ink);
            b.draw_colored_polygon(&poly(&[(0.37, 0.55), (0.63, 0.55), (0.6, 0.74), (0.4, 0.74)]), ink);
            b.draw_circle(pt(0.41, 0.46), size * 0.065, dark);
            b.draw_circle(pt(0.59, 0.46), size * 0.065, dark);
            b.draw_colored_polygon(&poly(&[(0.5, 0.53), (0.535, 0.6), (0.465, 0.6)]), dark);
            for x in [0.45, 0.5, 0.55] {
                b.draw_colored_polygon(&poly(&[(x - 0.008, 0.66), (x + 0.008, 0.66), (x + 0.008, 0.74), (x - 0.008, 0.74)]), dark);
            }
        }
        "menace" => {
            b.draw_colored_polygon(&poly(&[(0.34, 0.5), (0.2, 0.2), (0.44, 0.42)]), ink);
            b.draw_colored_polygon(&poly(&[(0.66, 0.5), (0.8, 0.2), (0.56, 0.42)]), ink);
            b.draw_circle(pt(0.5, 0.58), size * 0.21, ink);
            b.draw_colored_polygon(&poly(&[(0.35, 0.52), (0.47, 0.58), (0.45, 0.62), (0.36, 0.58)]), dark);
            b.draw_colored_polygon(&poly(&[(0.65, 0.52), (0.53, 0.58), (0.55, 0.62), (0.64, 0.58)]), dark);
            b.draw_colored_polygon(&poly(&[(0.42, 0.7), (0.58, 0.7), (0.56, 0.73), (0.44, 0.73)]), dark);
        }
        "trample" => {
            b.draw_colored_polygon(
                &poly(&[(0.18, 0.42), (0.54, 0.42), (0.54, 0.28), (0.82, 0.5), (0.54, 0.72), (0.54, 0.58), (0.18, 0.58)]),
                ink,
            );
        }
        "vigilance" => {
            let n = 20;
            let mut lens = PackedVector2Array::new();
            for i in 0..=n {
                let t = i as f32 / n as f32;
                lens.push(pt(0.18 + 0.64 * t, 0.5 - 0.2 * (PI * t).sin()));
            }
            for i in (1..n).rev() {
                let t = i as f32 / n as f32;
                lens.push(pt(0.18 + 0.64 * t, 0.5 + 0.2 * (PI * t).sin()));
            }
            b.draw_colored_polygon(&lens, ink);
            b.draw_circle(c, size * 0.12, dark);
            b.draw_circle(c, size * 0.045, ink);
        }
        "lifelink" => {
            b.draw_circle(pt(0.39, 0.42), size * 0.125, ink);
            b.draw_circle(pt(0.61, 0.42), size * 0.125, ink);
            b.draw_colored_polygon(&poly(&[(0.275, 0.47), (0.725, 0.47), (0.5, 0.77)]), ink);
        }
        "reach" => {
            b.draw_arc_ex(pt(0.5, 0.72), size * 0.27, PI * 1.12, PI * 1.88, 24, ink).width(size * 0.055).antialiased(true).done();
            b.draw_colored_polygon(
                &poly(&[(0.5, 0.15), (0.63, 0.33), (0.54, 0.33), (0.54, 0.82), (0.46, 0.82), (0.46, 0.33), (0.37, 0.33)]),
                ink,
            );
        }
        "haste" => {
            b.draw_colored_polygon(
                &poly(&[(0.58, 0.14), (0.3, 0.55), (0.48, 0.55), (0.4, 0.86), (0.7, 0.43), (0.52, 0.43), (0.64, 0.14)]),
                ink,
            );
        }
        "hexproof" | "ward" => {
            let shield = [(0.24, 0.22), (0.5, 0.15), (0.76, 0.22), (0.74, 0.52), (0.5, 0.85), (0.26, 0.52)];
            b.draw_colored_polygon(&poly(&shield), ink);
            let inner: Vec<(f32, f32)> = shield.iter().map(|&(x, y)| (0.5 + (x - 0.5) * 0.7, 0.49 + (y - 0.49) * 0.7)).collect();
            b.draw_colored_polygon(&poly(&inner), dark);
            if kind == "ward" {
                b.draw_circle(pt(0.5, 0.47), size * 0.08, ink);
            }
        }
        "indestructible" => {
            b.draw_colored_polygon(&poly(&[(0.5, 0.16), (0.8, 0.44), (0.5, 0.84), (0.2, 0.44)]), ink);
            b.draw_colored_polygon(&poly(&[(0.5, 0.3), (0.64, 0.44), (0.5, 0.64), (0.36, 0.44)]), dark);
        }
        "phasing" => {
            // Phasing (CR 702.26): gestrichelter Kreis mit Punkt
            for i in 0..8 {
                let a = i as f32 * TAU / 8.0;
                b.draw_arc_ex(c, size * 0.24, a, a + TAU / 16.0, 6, ink).width(size * 0.06).antialiased(true).done();
            }
            b.draw_circle(c, size * 0.08, ink);
        }
        "defender" => {
            // Mauer: drei Reihen Steine, versetzt
            let (x0, x1) = (0.2f32, 0.8f32);
            for (row, y) in [0.28f32, 0.45, 0.62].into_iter().enumerate() {
                let offset = if row % 2 == 0 { 0.0 } else { 0.1 };
                let mut x = x0 - offset;
                while x < x1 {
                    let (a, e) = (x.max(x0), (x + 0.19).min(x1));
                    if e - a > 0.03 {
                        b.draw_colored_polygon(&poly(&[(a, y), (e, y), (e, y + 0.14), (a, y + 0.14)]), ink);
                    }
                    x += 0.2;
                }
            }
        }
        _ => {
            b.draw_arc_ex(c, size * 0.2, 0.0, TAU, 24, ink).width(size * 0.05).antialiased(true).done();
        }
    }
}

/// Phasing-Symbol (ausgephaste Karte) als `Sprite3D` (Durchmesser `size`).
pub fn phasing_icon(size: f32) -> Gd<Sprite3D> {
    symbol_sprite("&phasing", size)
}

/// Schlüsselwort-Symbol als `Sprite3D` (Durchmesser `size`, Fläche in der XY-Ebene, Vorderseite +Z).
pub fn keyword_icon(icon: crate::game::cards::KeywordIcon, size: f32) -> Gd<Sprite3D> {
    symbol_sprite(&format!("&{}", icon.id()), size)
}

/// Ist `kind` ein Kennzeichen (Monarch, Initiative, Stadtsegen, Ring) statt einer Spieler-Marke?
/// Kennzeichen zeigt der Tisch wie Arena als freistehendes Emblem, Marken als Plakette mit Zahl.
pub fn is_seat_designation(kind: &str) -> bool {
    matches!(kind, "monarch" | "monarch_card" | "initiative" | "blessing" | "ring")
}

/// Kennzeichen bzw. Marke eines Spielers zeichnen, wie in Arena:
/// - Kennzeichen ([`is_seat_designation`]) als freistehendes, golden leuchtendes Emblem: `monarch`
///   (Krone mit Edelstein, CR 724), `initiative` (Wappenschild mit Schwert, CR 725), `blessing`
///   (goldene Stadt im Strahlenkranz, Stadtsegen CR 702.131), `ring` (der Eine Ring mit glühender
///   Inschrift, CR 701.54; `count` > 1 = wie oft versucht, als kleine Zahl).
/// - Marken als dunkle Plakette mit Symbol links und Anzahl rechts: `poison` (grünes Φ),
///   `energy` (oranger Blitz), `experience` (violetter Stern), sonst der Anfangsbuchstabe.
fn draw_seat_token(b: &mut godot::classes::CanvasItem, kind: &str, count: i64, size: f32) {
    use std::f32::consts::{PI, TAU};
    let c = Vector2::splat(size * 0.5);
    let poly = |pts: &[Vector2]| PackedVector2Array::from(pts);
    let dark = Color::from_rgba(0.05, 0.03, 0.02, 0.92);
    let gold = Color::from_rgb(1.0, 0.8, 0.32);
    let gold_light = Color::from_rgb(1.0, 0.94, 0.62);
    // Umriss: Fläche vergrößert dunkel, dann farbig (wie Arenas Embleme mit dunkler Kontur)
    let outlined = |b: &mut godot::classes::CanvasItem, pts: &[Vector2], fill: Color, k: f32| {
        let mid = pts.iter().fold(Vector2::ZERO, |a, p| a + *p) / pts.len() as f32;
        let big: Vec<Vector2> = pts.iter().map(|p| mid + (*p - mid) * k).collect();
        b.draw_colored_polygon(&poly(&big), dark);
        b.draw_colored_polygon(&poly(pts), fill);
    };
    let font = ThemeDb::singleton().get_fallback_font();
    let text_at = |b: &mut godot::classes::CanvasItem, text: &str, center: Vector2, width: f32, fs: i32, color: Color| {
        if let Some(font) = font.as_ref() {
            let ascent = font.get_ascent_ex().font_size(fs).done();
            let descent = font.get_descent_ex().font_size(fs).done();
            let pos = Vector2::new(center.x - width * 0.5, center.y + (ascent - descent) * 0.5);
            b.draw_string_outline_ex(font, pos, text).alignment(HorizontalAlignment::CENTER).width(width).font_size(fs).size((fs / 6).max(2)).modulate(dark).done();
            b.draw_string_ex(font, pos, text).alignment(HorizontalAlignment::CENTER).width(width).font_size(fs).modulate(color).done();
        }
    };
    if is_seat_designation(kind) {
        // Leuchten hinter dem Emblem: weiche Scheiben nach außen schwächer
        let glow = match kind {
            "initiative" => Color::from_rgb(1.0, 0.45, 0.25),
            "ring" => Color::from_rgb(1.0, 0.5, 0.15),
            _ => Color::from_rgb(1.0, 0.8, 0.35),
        };
        for i in 0..6 {
            let k = 1.0 - i as f32 / 6.0;
            b.draw_circle(c, size * (0.18 + 0.3 * k), Color::from_rgba(glow.r, glow.g, glow.b, 0.07));
        }
        let s = size * 0.3;
        let p = |x: f32, y: f32| c + Vector2::new(x, y) * s;
        match kind {
            "monarch" => {
                // Krone: Reif mit fünf Zacken, Kugeln auf den Spitzen, roter Stein in der Mitte
                let crown = [p(-1.15, 0.7), p(1.15, 0.7), p(1.2, -0.55), p(0.62, -0.05), p(0.38, -0.95), p(0.0, -0.2), p(-0.38, -0.95), p(-0.62, -0.05), p(-1.2, -0.55)];
                outlined(b, &crown, gold, 1.14);
                b.draw_colored_polygon(&poly(&[p(-1.1, 0.42), p(1.1, 0.42), p(1.12, 0.7), p(-1.12, 0.7)]), Color::from_rgb(0.82, 0.55, 0.16));
                b.draw_colored_polygon(&poly(&[p(-0.9, 0.3), p(-0.2, -0.05), p(-0.35, 0.3)]), gold_light);
                for (x, y) in [(-1.2f32, -0.62f32), (-0.38, -1.02), (0.38, -1.02), (1.2, -0.62)] {
                    b.draw_circle(p(x, y), s * 0.17, dark);
                    b.draw_circle(p(x, y), s * 0.12, gold_light);
                }
                b.draw_circle(p(0.0, 0.2), s * 0.24, dark);
                b.draw_circle(p(0.0, 0.2), s * 0.18, Color::from_rgb(0.85, 0.12, 0.18));
                b.draw_circle(p(-0.05, 0.14), s * 0.06, Color::from_rgb(1.0, 0.7, 0.7));
            }
            "initiative" => {
                // Wappenschild (rot, goldener Rand) mit aufrechtem Schwert
                let shield = [p(-1.0, -1.05), p(1.0, -1.05), p(1.0, 0.1), p(0.0, 1.2), p(-1.0, 0.1)];
                outlined(b, &shield, gold, 1.1);
                let inner: Vec<Vector2> = shield.iter().map(|q| c + (*q - c) * 0.8 + Vector2::new(0.0, -s * 0.02)).collect();
                b.draw_colored_polygon(&poly(&inner), Color::from_rgb(0.62, 0.1, 0.1));
                b.draw_colored_polygon(&poly(&[p(0.0, -0.95), p(0.12, -0.75), p(0.12, 0.45), p(-0.12, 0.45), p(-0.12, -0.75)]), Color::from_rgb(0.92, 0.92, 0.95));
                b.draw_rect(Rect2::new(p(-0.45, 0.4), Vector2::new(s * 0.9, s * 0.14)), gold_light);
                b.draw_rect(Rect2::new(p(-0.07, 0.54), Vector2::new(s * 0.14, s * 0.3)), gold);
                b.draw_circle(p(0.0, 0.88), s * 0.1, gold_light);
            }
            "blessing" => {
                // Strahlenkranz, darin die goldene Stadt (Stufenpyramide mit zwei Türmen)
                let rays: Vec<Vector2> = (0..24)
                    .map(|i| {
                        let a = -PI * 0.5 + i as f32 * TAU / 24.0;
                        let rr = if i % 2 == 0 { 1.3 } else { 0.95 };
                        p(a.cos() * rr, a.sin() * rr)
                    })
                    .collect();
                outlined(b, &rays, Color::from_rgb(0.95, 0.72, 0.28), 1.06);
                b.draw_circle(c, s * 0.92, dark);
                b.draw_circle(c, s * 0.84, Color::from_rgb(0.98, 0.9, 0.6));
                let city = [
                    p(-0.75, 0.55), p(0.75, 0.55), p(0.75, 0.25), p(0.55, 0.25), p(0.55, -0.35), p(0.42, -0.6), p(0.3, -0.35),
                    p(0.3, -0.05), p(0.18, -0.05), p(0.18, -0.5), p(0.0, -0.8), p(-0.18, -0.5), p(-0.18, -0.05), p(-0.3, -0.05),
                    p(-0.3, -0.35), p(-0.42, -0.6), p(-0.55, -0.35), p(-0.55, 0.25), p(-0.75, 0.25),
                ];
                b.draw_colored_polygon(&poly(&city), Color::from_rgb(0.72, 0.48, 0.12));
                b.draw_rect(Rect2::new(p(-0.08, 0.2), Vector2::new(s * 0.16, s * 0.35)), Color::from_rgb(0.3, 0.18, 0.05));
            }
            _ => {
                // Ring: goldener Reif in leichter Aufsicht, glühende Inschrift
                let ring_pts = |rx: f32, ry: f32| -> PackedVector2Array { (0..=48).map(|i| { let a = i as f32 * TAU / 48.0; p(a.cos() * rx, a.sin() * ry + 0.05) }).collect() };
                let band = ring_pts(0.95, 0.66);
                b.draw_polyline_ex(&band, dark).width(s * 0.52).antialiased(true).done();
                b.draw_polyline_ex(&band, gold).width(s * 0.38).antialiased(true).done();
                // Inschrift: kurze glühende Striche entlang der Mitte des Reifs (nur vorn sichtbar)
                for i in 0..14 {
                    let a = 0.15 * PI + i as f32 * 0.7 * PI / 13.0;
                    let q = p(a.cos() * 0.95, a.sin() * 0.65 + 0.05);
                    b.draw_line_ex(q - Vector2::new(s * 0.05, 0.0), q + Vector2::new(s * 0.05, -s * 0.03), Color::from_rgb(1.0, 0.45, 0.1)).width(s * 0.07).antialiased(true).done();
                }
                b.draw_arc_ex(p(0.0, 0.05), s * 0.98, 1.1 * PI, 1.6 * PI, 16, gold_light).width(s * 0.08).antialiased(true).done();
                if count > 1 {
                    text_at(b, &count.to_string(), p(0.0, 0.05), s * 1.2, (size * 0.24) as i32, gold_light);
                }
            }
        }
        return;
    }
    // Marke: dunkle Plakette, Symbol links im farbigen Kreis, Anzahl rechts
    let (accent, ink) = match kind {
        "poison" => (Color::from_rgb(0.35, 0.85, 0.3), Color::from_rgb(0.1, 0.2, 0.08)),
        "energy" => (Color::from_rgb(1.0, 0.62, 0.15), Color::from_rgb(0.25, 0.12, 0.02)),
        "experience" => (Color::from_rgb(0.72, 0.5, 1.0), Color::from_rgb(0.16, 0.08, 0.28)),
        _ => (Color::from_rgb(0.8, 0.8, 0.85), Color::from_rgb(0.15, 0.15, 0.18)),
    };
    let h = size * 0.46;
    let top = c.y - h * 0.5;
    let (x0, x1) = (size * 0.04, size * 0.96);
    let rounded = |b: &mut godot::classes::CanvasItem, grow: f32, col: Color, dy: f32| {
        let r = h * 0.5 + grow;
        b.draw_rect(Rect2::new(Vector2::new(x0 + h * 0.5, top - grow + dy), Vector2::new(x1 - x0 - h, h + 2.0 * grow)), col);
        b.draw_circle(Vector2::new(x0 + h * 0.5, c.y + dy), r, col);
        b.draw_circle(Vector2::new(x1 - h * 0.5, c.y + dy), r, col);
    };
    rounded(b, size * 0.02, Color::from_rgba(0.0, 0.0, 0.0, 0.5), size * 0.025);
    rounded(b, size * 0.02, accent, 0.0);
    rounded(b, 0.0, Color::from_rgb(0.1, 0.1, 0.12), 0.0);
    let ic = Vector2::new(x0 + h * 0.5, c.y);
    let ir = h * 0.62;
    b.draw_circle(ic, ir + size * 0.015, dark);
    b.draw_circle(ic, ir, accent);
    let s = ir * 0.62;
    let p = |x: f32, y: f32| ic + Vector2::new(x, y) * s;
    match kind {
        "poison" => {
            // Φ (phyrexianisch)
            b.draw_arc_ex(ic, s * 0.55, 0.0, TAU, 32, ink).width(s * 0.22).antialiased(true).done();
            b.draw_rect(Rect2::new(p(-0.1, -1.0), Vector2::new(s * 0.2, s * 2.0)), ink);
        }
        "energy" => {
            b.draw_colored_polygon(&poly(&[p(0.3, -1.0), p(-0.6, 0.12), p(-0.05, 0.12), p(-0.3, 1.0), p(0.6, -0.15), p(0.05, -0.15)]), ink);
        }
        "experience" => {
            let pts: Vec<Vector2> = (0..10)
                .map(|i| {
                    let a = -PI * 0.5 + i as f32 * PI / 5.0;
                    let rr = if i % 2 == 0 { 1.0 } else { 0.45 };
                    p(a.cos() * rr, a.sin() * rr)
                })
                .collect();
            b.draw_colored_polygon(&poly(&pts), ink);
        }
        other => {
            let letter: String = other.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_default();
            text_at(b, &letter, ic, ir * 2.0, (size * 0.24) as i32, ink);
        }
    }
    let text = count.to_string();
    let fs = if text.len() <= 2 { (size * 0.3) as i32 } else { (size * 0.22) as i32 };
    text_at(b, &text, Vector2::new((ic.x + ir + x1) * 0.5, c.y), x1 - ic.x - ir, fs, Color::from_rgb(0.97, 0.97, 0.97));
}

/// Kennzeichen bzw. Marke eines Spielers als `Sprite3D` (Kantenlänge `size`, Fläche in der
/// XY-Ebene, Vorderseite +Z); `kind`/`count` siehe [`draw_seat_token`].
pub fn seat_token(kind: &str, count: i64, size: f32) -> Gd<Sprite3D> {
    symbol_sprite(&format!("§{kind}:{count}"), size)
}

/// Token des Kosten-Symbols einer Loyalitätsfähigkeit („+1“ → `%up:+1`, „-3“/„−3“ → `%down:−3`,
/// „0“ → `%zero:0`); None, wenn `cost` keine Loyalitätskosten sind.
pub fn loyalty_cost_token(cost: &str) -> Option<String> {
    let c = cost.trim().replace('-', "−");
    let digits = c.trim_start_matches(['+', '−']);
    if digits.is_empty() || !digits.chars().all(|ch| ch.is_ascii_digit() || ch == 'X') {
        return None;
    }
    Some(if c.starts_with('+') {
        format!("%up:{c}")
    } else if c.starts_with('−') {
        format!("%down:{c}")
    } else {
        format!("%zero:{c}")
    })
}

/// Loyalitäts-Schild als `Sprite3D` (Höhe `size`, Fläche in der XY-Ebene, Vorderseite +Z) mit
/// der Loyalität `value`.
pub fn loyalty_shield(value: i32, size: f32) -> Gd<Sprite3D> {
    symbol_sprite(&format!("%shield:{value}"), size)
}

thread_local! {
    /// Je Symbol seine `SubViewport` (Knoten, kein Referenzzähler: beim Beenden räumt Godot sie
    /// mit dem Baum ab, der Zwischenspeicher hält keine Textur fest).
    static CACHE: RefCell<HashMap<String, Gd<SubViewport>>> = RefCell::new(HashMap::new());
}

/// Textur eines Symbols (einmal gezeichnet, danach aus dem Zwischenspeicher). Die
/// `SubViewport` hängt unsichtbar an der Wurzel des Szenenbaums.
pub fn symbol_texture(token: &str) -> Option<Gd<Texture2D>> {
    // Symbolbogen (`game::mana_symbols`) für alle Manasymbole, Zahlen, X, Hybride …
    if !token.starts_with('#') && !token.starts_with('&') {
        if let Some(t) = crate::game::mana_symbols::texture(token) {
            return Some(t);
        }
    }
    if let Some(vp) = CACHE.with(|c| c.borrow().get(token).cloned()).filter(|v| v.is_instance_valid()) {
        return vp.get_texture().map(|t| t.upcast::<Texture2D>());
    }
    let tree = Engine::singleton().get_main_loop()?.try_cast::<SceneTree>().ok()?;
    let mut root = tree.get_root();
    let mut vp = SubViewport::new_alloc();
    vp.set_name(&format!("CostSymbol_{}", token.replace(['/', '#', ':', '%', '+', '§', '&'], "_")));
    vp.set_size(Vector2i::new(TEX, TEX));
    vp.set_transparent_background(true);
    vp.set_disable_3d(true);
    vp.set_update_mode(UpdateMode::ONCE);
    let mut canvas = CostSymbolCanvas::new_alloc();
    canvas.bind_mut().token = token.to_string();
    canvas.set_size(Vector2::splat(TEX as f32));
    canvas.set_mouse_filter(MouseFilter::IGNORE);
    vp.add_child(&canvas);
    root.call_deferred("add_child", &[vp.to_variant()]);
    let tex = vp.get_texture()?.upcast::<Texture2D>();
    CACHE.with(|c| c.borrow_mut().insert(token.to_string(), vp));
    Some(tex)
}

thread_local! {
    /// Fließtext mit Symbolen: je (Text, Größe, Farbe) seine `SubViewport`.
    static RICH: RefCell<HashMap<String, Gd<SubViewport>>> = RefCell::new(HashMap::new());
}

/// Fließtext mit eingebetteten Manasymbolen („{T}: Add {R}.“) als Textur `size` Pixel groß
/// (umbrochen, mittig), Schrift `font_px` in `color` – z. B. Regeltext auf Karten, den ein
/// `Label3D` nicht mit Bildern setzen kann. Einmal gerendert und zwischengespeichert.
pub fn rich_text_texture(text: &str, font_px: i32, size: Vector2i, color: Color) -> Option<Gd<Texture2D>> {
    let key = format!("{text}|{font_px}|{}x{}|{}", size.x, size.y, color.to_html());
    if let Some(vp) = RICH.with(|c| c.borrow().get(&key).cloned()).filter(|v| v.is_instance_valid()) {
        return vp.get_texture().map(|t| t.upcast::<Texture2D>());
    }
    let tree = Engine::singleton().get_main_loop()?.try_cast::<SceneTree>().ok()?;
    let mut root = tree.get_root();
    let mut vp = SubViewport::new_alloc();
    vp.set_size(size);
    vp.set_transparent_background(true);
    vp.set_disable_3d(true);
    vp.set_update_mode(UpdateMode::ONCE);
    let mut label = RichTextLabel::new_alloc();
    label.set_size(Vector2::new(size.x as f32, size.y as f32));
    label.set_scroll_active(false);
    label.set_fit_content(false);
    label.set_horizontal_alignment(HorizontalAlignment::CENTER);
    label.set_vertical_alignment(godot::global::VerticalAlignment::CENTER);
    label.set_mouse_filter(MouseFilter::IGNORE);
    label.add_theme_font_size_override("normal_font_size", font_px);
    label.add_theme_color_override("default_color", color);
    crate::game::mana_symbols::fill_rich(&mut label, text, (font_px as f32 * 0.95) as i32);
    vp.add_child(&label);
    root.call_deferred("add_child", &[vp.to_variant()]);
    let tex = vp.get_texture()?.upcast::<Texture2D>();
    RICH.with(|c| c.borrow_mut().insert(key, vp));
    Some(tex)
}

/// Ein Symbol als `Sprite3D` (Fläche in der XY-Ebene, Vorderseite +Z) mit Höhe `height`.
fn symbol_sprite(token: &str, height: f32) -> Gd<Sprite3D> {
    let mut s = Sprite3D::new_alloc();
    if let Some(t) = symbol_texture(token) {
        s.set_texture(&t);
    }
    s.set_pixel_size(height / TEX as f32);
    s.set_billboard_mode(BillboardMode::DISABLED);
    s.set_draw_flag(SpriteFlags::SHADED, false);
    s.set_draw_flag(SpriteFlags::DOUBLE_SIDED, false);
    s.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
    s.set_render_priority(3);
    s
}

/// Marken-Münze als `Sprite3D` (Durchmesser `size`, Fläche in der XY-Ebene, Vorderseite +Z):
/// silbern mit der Anzahl, Rand je Markenart (+1/+1 grün, −1/−1 rot, sonst golden) – wie im
/// Video „Choose One + Marke“. Sonderfälle: `CLASS_LEVEL` („L2“), `DOORS` („1/2“), `PREPARED`
/// (blauer Rand, „V“ – die Kreatur ist vorbereitet und ihr Zauber liegt als Kopie neben der Hand).
pub fn counter_coin(kind: &str, count: u32, size: f32) -> Gd<Sprite3D> {
    let rim = match kind {
        "P1P1" => "green",
        "M1M1" => "red",
        "PREPARED" => "blue",
        _ => "gold",
    };
    // Klassen-Stufe „L2“, offene Türen „1/2“, vorbereitet „V“/„P“, sonst die Anzahl
    let label = match kind {
        "PREPARED" => crate::tr!("V", "P").to_string(),
        "CLASS_LEVEL" => format!("L{count}"),
        "DOORS" => format!("{count}/2"),
        _ => count.to_string(),
    };
    symbol_sprite(&format!("#{rim}:{label}"), size)
}

/// Breite einer Symbolreihe aus `n` Symbolen der Höhe `height`.
pub fn row_width(n: usize, height: f32) -> f32 {
    if n == 0 {
        return 0.0;
    }
    n as f32 * height + (n - 1) as f32 * height * SYMBOL_GAP
}

/// Symbolreihe (Fläche in der XY-Ebene, Vorderseite +Z), um den Ursprung zentriert.
pub fn cost_row(tokens: &[String], height: f32) -> Gd<Node3D> {
    let mut row = Node3D::new_alloc();
    row.set_name("CostRow");
    let width = row_width(tokens.len(), height);
    for (i, t) in tokens.iter().enumerate() {
        let mut s = symbol_sprite(t, height);
        let x = -width * 0.5 + height * 0.5 + i as f32 * height * (1.0 + SYMBOL_GAP);
        s.set_position(Vector3::new(x, 0.0, 0.0));
        row.add_child(&s);
    }
    row
}

/// Enthält der Text Kosten in geschweiften Klammern („Bezahlen {1}{U}“, „+ {3}“)?
pub fn has_cost(text: &str) -> bool {
    text.contains('{') && text.contains('}')
}

/// Teil einer Beschriftung: Text oder Kostensymbole.
enum Part {
    Text(String),
    Symbols(Vec<String>),
}

fn parts(text: &str) -> Vec<Part> {
    let mut out = Vec::new();
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        let before = rest[..open].trim();
        if !before.is_empty() {
            out.push(Part::Text(before.to_string()));
        }
        // zusammenhängende {…}{…} bilden eine Symbolreihe
        let mut end = open;
        let mut tokens = Vec::new();
        while rest[end..].starts_with('{') {
            let Some(close) = rest[end..].find('}') else { break };
            tokens.push(rest[end + 1..end + close].trim().to_uppercase());
            end += close + 1;
        }
        if tokens.is_empty() {
            out.push(Part::Text(rest[open..].to_string()));
            return out;
        }
        out.push(Part::Symbols(tokens));
        rest = &rest[end..];
    }
    if !rest.trim().is_empty() {
        out.push(Part::Text(rest.trim().to_string()));
    }
    out
}

/// Breite von `text` als `Label3D` mit `font_size` und `pixel_size` (Welteinheiten).
fn text_width(text: &str, font_size: i32, pixel_size: f32) -> f32 {
    ThemeDb::singleton()
        .get_fallback_font()
        .map_or(text.chars().count() as f32 * font_size as f32 * 0.55, |f| {
            f.get_string_size_ex(text).alignment(HorizontalAlignment::LEFT).font_size(font_size).done().x
        })
        * pixel_size
}

/// Breite einer Beschriftung mit Kostensymbolen bei Schriftgröße `font_size` und `pixel_size`.
pub fn rich_width(text: &str, font_size: i32, pixel_size: f32) -> f32 {
    let sym = font_size as f32 * pixel_size * SYMBOL_PER_FONT;
    let ps = parts(text);
    let gaps = ps.len().saturating_sub(1) as f32 * sym * TEXT_GAP;
    ps.iter()
        .map(|p| match p {
            Part::Text(t) => text_width(t, font_size, pixel_size),
            Part::Symbols(s) => row_width(s.len(), sym),
        })
        .sum::<f32>()
        + gaps
}

/// Beschriftung mit Kostensymbolen statt „{…}“ (Fläche in der XY-Ebene, Vorderseite +Z, um den
/// Ursprung zentriert): Textteile als `Label3D` im Stil von `style` (Schrift, Farbe, Umriss werden
/// übernommen), Kosten als Symbolreihe in Schrifthöhe.
pub fn rich_label(text: &str, style: &Gd<Label3D>) -> Gd<Node3D> {
    let font_size = style.get_font_size();
    let pixel_size = style.get_pixel_size();
    let sym = font_size as f32 * pixel_size * SYMBOL_PER_FONT;
    let mut node = Node3D::new_alloc();
    node.set_name("RichLabel");
    let total = rich_width(text, font_size, pixel_size);
    let mut x = -total * 0.5;
    for p in parts(text) {
        match p {
            Part::Text(t) => {
                let w = text_width(&t, font_size, pixel_size);
                let mut l = style.duplicate_node().try_cast::<Label3D>().unwrap_or_else(|_| Label3D::new_alloc());
                l.set_name("Text");
                l.set_text(&t);
                // Farbe ohne die gerade laufende Ein-/Ausblende-Transparenz der Vorlage übernehmen
                let c = l.get_modulate();
                l.set_modulate(Color::from_rgba(c.r, c.g, c.b, 1.0));
                l.set_visible(true);
                l.set_draw_flag(LabelFlags::SHADED, false);
                l.set_position(Vector3::new(x + w * 0.5, 0.0, 0.0));
                node.add_child(&l);
                x += w + sym * TEXT_GAP;
            }
            Part::Symbols(tokens) => {
                let w = row_width(tokens.len(), sym);
                let mut row = cost_row(&tokens, sym);
                // Symbole sitzen etwas tiefer als die Schriftmitte (wie Großbuchstaben)
                row.set_position(Vector3::new(x + w * 0.5, -sym * 0.04, 0.002));
                node.add_child(&row);
                x += w + sym * TEXT_GAP;
            }
        }
    }
    node
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_costs_into_tokens() {
        assert_eq!(cost_tokens("{2}{W}{W}"), vec!["2", "W", "W"]);
        assert_eq!(cost_tokens("1 U"), vec!["1", "U"]);
        assert_eq!(cost_tokens("X r/p"), vec!["X", "R/P"]);
        assert!(cost_tokens("").is_empty());
    }

    #[test]
    fn splits_labels_into_text_and_symbols() {
        let p = parts("Bezahlen {1}{U}");
        assert!(matches!(&p[0], Part::Text(t) if t == "Bezahlen"));
        assert!(matches!(&p[1], Part::Symbols(s) if s == &vec!["1".to_string(), "U".to_string()]));
        let p = parts("Kicker – {R} extra");
        assert_eq!(p.len(), 3);
        assert!(has_cost("+ {3}"));
        assert!(!has_cost("Normal"));
    }
}
