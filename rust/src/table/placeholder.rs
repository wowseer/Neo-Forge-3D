//! Erzeugt Platzhalter-Texturen für Karten (Rahmen, Kostensymbole, Typsymbol), bis echte
//! Kartenbilder existieren. Texte kommen per Label3D dazu.

use godot::classes::image::Format;
use godot::classes::{Image, ImageTexture, Texture2D};
use std::cell::RefCell;
use std::collections::HashMap;

use godot::prelude::*;

use crate::game::cards::{CardDef, CardKind};
use crate::game::mana::ManaType;

const W: i32 = 256;
const H: i32 = 358;

fn finish(mut img: Gd<Image>) -> Gd<Texture2D> {
    img.generate_mipmaps();
    ImageTexture::create_from_image(&img)
        .expect("ImageTexture aus Image")
        .upcast()
}

fn rect(x: i32, y: i32, w: i32, h: i32) -> Rect2i {
    Rect2i::new(Vector2i::new(x, y), Vector2i::new(w, h))
}

/// Durchmesser und Abstand der Kostensymbole in der Titelleiste (Pixel).
const PIP: i32 = 24;
const PIP_STEP: i32 = 26;
const TITLE_Y: i32 = 36;

/// Pixelmitten (x) der Kostensymbole, von links nach rechts, rechtsbündig in der Titelleiste.
pub fn pip_centers(count: usize) -> Vec<f32> {
    (0..count)
        .map(|i| (W - 34 - (count - 1 - i) as i32 * PIP_STEP) as f32)
        .collect()
}

/// Kostensymbole eines Kartenbilds in Reihenfolge: generische Zahl (falls > 0), dann je
/// farbiges Mana sein Buchstabe (W U B R G C).
pub fn pip_tokens(def: &CardDef) -> Vec<String> {
    let mut out = Vec::new();
    if def.cost.generic > 0 {
        out.push(def.cost.generic.to_string());
    }
    for t in ManaType::ALL {
        let letter = match t {
            ManaType::White => "W",
            ManaType::Blue => "U",
            ManaType::Black => "B",
            ManaType::Red => "R",
            ManaType::Green => "G",
            ManaType::Colorless => "C",
        };
        for _ in 0..def.cost.specific.get(t) {
            out.push(letter.to_string());
        }
    }
    out
}

/// Zeigt das Kartenbild die generische Zahl selbst (Symbol aus dem Bogen)? Dann braucht es
/// keine Beschriftung darüber.
pub fn draws_generic(def: &CardDef) -> bool {
    def.cost.generic > 0 && crate::game::mana_symbols::cell(&def.cost.generic.to_string()).is_some()
}

/// Pixelmitte (y) der Titelleiste.
pub fn title_center_y() -> f32 {
    TITLE_Y as f32
}

/// Gefüllter Kreis (zeilenweise aus Rechtecken).
fn circle(img: &mut Gd<Image>, cx: i32, cy: i32, r: i32, color: Color) {
    for dy in -r..=r {
        let half = (((r * r - dy * dy) as f32).sqrt()) as i32;
        img.fill_rect(rect(cx - half, cy + dy, half * 2 + 1, 1), color);
    }
}

thread_local! {
    /// Geladene Kartenbilder je Pfad (None = nicht ladbar).
    static PRINTED: RefCell<HashMap<String, Option<Gd<Texture2D>>>> = RefCell::new(HashMap::new());
    /// Nach [`clear_printed`] (Tisch wird abgebaut): nichts mehr laden und zwischenspeichern,
    /// sonst hielte der Speicher Texturen über das Ende der Engine hinaus (Absturz beim Beenden).
    static CLOSED: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

/// Kartenbild aus einer Datei (JPG/PNG von Scryfall, Pfad aus dem Zustand), einmal geladen und
/// zwischengespeichert; None, wenn die Datei fehlt oder nicht lesbar ist.
pub fn printed_face(path: &str) -> Option<Gd<Texture2D>> {
    if path.is_empty() || CLOSED.with(|c| c.get()) {
        return None;
    }
    if let Some(t) = PRINTED.with(|c| c.borrow().get(path).cloned()) {
        return t;
    }
    let tex = Image::load_from_file(path).map(|mut img| {
        img.generate_mipmaps();
        ImageTexture::create_from_image(&img).map(|t| t.upcast::<Texture2D>())
    });
    let tex = tex.flatten();
    PRINTED.with(|c| c.borrow_mut().insert(path.to_string(), tex.clone()));
    tex
}

/// Zwischenspeicher der Kartenbilder leeren (beim Verlassen des Tisches, damit beim Beenden
/// keine Texturen übrig bleiben).
pub fn clear_printed() {
    CLOSED.with(|c| c.set(true));
    PRINTED.with(|c| c.borrow_mut().clear());
    TILES.with(|c| c.borrow_mut().clear());
    super::card3d::clear_stats_plate_cache();
}

/// Zwischenspeicher wieder öffnen (der Tisch wird neu aufgebaut).
pub fn open_printed() {
    CLOSED.with(|c| c.set(false));
}

/// Bühnenkopie mit eigenem Text einer Karte ohne Druck mit normalem Rahmen: dann gezeichneter
/// Rahmen mit dem Bildausschnitt statt des Kartenbilds (dessen Textfeld liegt woanders).
fn own_frame(def: &CardDef) -> bool {
    def.custom_text && !def.art.is_empty()
}

/// Zeigt die Karte ihr echtes Kartenbild (statt des gezeichneten Platzhalters)?
pub fn has_printed_face(def: &CardDef) -> bool {
    // gezeichnete Dungeon-Karte: Name und Räume stehen im Bild
    if def.dungeon.is_some() && !def.face_down {
        return true;
    }
    !def.face_down && !own_frame(def) && printed_face(&def.image).is_some()
}

/// Vorderseite für eine Kartendefinition: das echte Kartenbild, wenn es eins gibt
/// ([`printed_face`], Name und Text stehen dann im Bild); sonst der Platzhalter mit Rahmen in
/// Kartenfarbe, Titelleiste mit Kostensymbolen, Bildfeld, Typzeile, Textfeld und (bei
/// Kreaturen) Wertefeld – Texte kommen dann per Label3D dazu.
pub fn card_face(def: &CardDef) -> Gd<Texture2D> {
    if def.face_down {
        return face_down_face(def);
    }
    // Dungeon: selbst gezeichnete Karte mit Räumen und aktuellem Raum (wie in Arena)
    if let Some(t) = def.dungeon.as_ref().and_then(crate::table::dungeon_view::dungeon_texture) {
        return t;
    }
    if !own_frame(def) {
        if let Some(t) = printed_face(&def.image) {
            return t;
        }
    }
    // Bildausschnitt fürs Bildfeld (nur eigener Rahmen bei Sonderrahmen-Karten)
    let art_image = if own_frame(def) { Image::load_from_file(&def.art) } else { None };
    let base = def.frame_color();
    let frame = base * 0.85;
    let dark = base.darkened(0.6);
    let art = base.lightened(0.45);

    let mut img = Image::create_empty(W, H, false, Format::RGBA8).expect("Image");
    img.fill(Color::from_rgb(0.08, 0.08, 0.1));
    img.fill_rect(rect(10, 10, W - 20, H - 20), frame);
    img.fill_rect(rect(20, 20, W - 40, 32), dark); // Titel
    img.fill_rect(rect(20, 60, W - 40, 140), art); // Bild
    img.fill_rect(rect(20, 208, W - 40, 22), dark); // Typzeile
    img.fill_rect(rect(20, 238, W - 40, 76), Color::from_rgb(0.93, 0.9, 0.82)); // Text (über dem Wertefeld)

    match def.kind {
        CardKind::Land => {
            // Großes Farbsymbol im Bildfeld
            let c = def.produces.color();
            circle(&mut img, W / 2, 130, 52, Color::from_rgb(0.1, 0.08, 0.06));
            circle(&mut img, W / 2, 130, 48, c);
            circle(&mut img, W / 2 - 14, 116, 14, c.lightened(0.45));
        }
        kind => {
            // Kostensymbole aus dem Symbolbogen (`game::mana_symbols`): zuerst die generische
            // Zahl, dann die farbigen; ohne Bogen (bzw. farblos) Farbkreise, die Zahl per Label3D
            for (x, token) in pip_centers(pip_tokens(def).len()).into_iter().zip(pip_tokens(def)) {
                circle(&mut img, x as i32, TITLE_Y, PIP / 2 + 1, Color::from_rgb(0.05, 0.04, 0.03));
                match crate::game::mana_symbols::image(&token) {
                    Some(mut sym) => {
                        sym.resize_ex(PIP, PIP).interpolation(godot::classes::image::Interpolation::LANCZOS).done();
                        img.blend_rect(&sym, rect(0, 0, PIP, PIP), Vector2i::new(x as i32 - PIP / 2, TITLE_Y - PIP / 2));
                    }
                    None => {
                        let c = token.chars().next().and_then(ManaType::from_wubrg).unwrap_or(ManaType::Colorless).color();
                        circle(&mut img, x as i32, TITLE_Y, PIP / 2 - 1, c);
                    }
                }
            }
            // Bildfeld: Bildausschnitt der Karte, sonst Typsymbol
            match art_image.clone() {
                Some(a) => blit_art(&mut img, a),
                None => emblem(&mut img, kind, dark),
            }
            // Wertefeld unten rechts (Kreatur, Planeswalker, Schlacht)
            if matches!(kind, CardKind::Creature | CardKind::Planeswalker | CardKind::Battle) {
                img.fill_rect(rect(W - 82, 318, 64, 32), Color::from_rgb(0.12, 0.08, 0.06));
                img.fill_rect(rect(W - 80, 320, 60, 28), dark);
            }
        }
    }
    finish(img)
}

/// Bildausschnitt in das Bildfeld (20/60, 216 × 140 px) einpassen: auf die Feldbreite skaliert,
/// überstehende Höhe oben und unten gleichmäßig abgeschnitten (bzw. umgekehrt bei breiten Bildern).
fn blit_art(img: &mut Gd<Image>, mut art: Gd<Image>) {
    let (fw, fh) = (W - 40, 140);
    let (aw, ah) = (art.get_width().max(1) as f32, art.get_height().max(1) as f32);
    let scale = (fw as f32 / aw).max(fh as f32 / ah);
    let (nw, nh) = ((aw * scale).ceil() as i32, (ah * scale).ceil() as i32);
    art.convert(Format::RGBA8);
    art.resize_ex(nw, nh).interpolation(godot::classes::image::Interpolation::LANCZOS).done();
    let src = rect((nw - fw) / 2, (nh - fh) / 2, fw, fh);
    img.blit_rect(&art, src, Vector2i::new(20, 60));
}

/// Einfaches Symbol je Kartentyp in der Mitte des Bildfelds (Mitte 128/130).
fn emblem(img: &mut Gd<Image>, kind: CardKind, c: Color) {
    let (cx, cy) = (W / 2, 130);
    match kind {
        CardKind::Instant => {
            // Blitz: versetzte Balken
            for (i, dx) in [8, 0, -8, 4, -4, -12].into_iter().enumerate() {
                img.fill_rect(rect(cx + dx - 6, cy - 48 + i as i32 * 16, 14, 18), c);
            }
        }
        CardKind::Sorcery => {
            // Zauberkreis: Ring mit Punkt
            circle(img, cx, cy, 46, c);
            circle(img, cx, cy, 38, c.lightened(0.85));
            circle(img, cx, cy, 10, c);
        }
        CardKind::Planeswalker => {
            // Krone: Sockel und drei Zacken
            img.fill_rect(rect(cx - 44, cy + 10, 88, 22), c);
            for (dx, h) in [(-38, 40), (-8, 56), (22, 40)] {
                img.fill_rect(rect(cx + dx, cy + 10 - h, 16, h), c);
                circle(img, cx + dx + 8, cy + 6 - h, 8, c);
            }
        }
        CardKind::Equipment => {
            // Schwert: Klinge, Parierstange, Griff
            img.fill_rect(rect(cx - 6, cy - 54, 12, 74), c);
            img.fill_rect(rect(cx - 30, cy + 20, 60, 10), c);
            img.fill_rect(rect(cx - 5, cy + 30, 10, 22), c.darkened(0.3));
            circle(img, cx, cy + 56, 7, c);
        }
        CardKind::Artifact => {
            // Zahnrad: Ring mit Zähnen
            for (dx, dy) in [(0, -46), (0, 46), (-46, 0), (46, 0), (-33, -33), (33, 33), (-33, 33), (33, -33)] {
                img.fill_rect(rect(cx + dx - 9, cy + dy - 9, 18, 18), c);
            }
            circle(img, cx, cy, 40, c);
            circle(img, cx, cy, 16, c.lightened(0.85));
        }
        CardKind::Enchantment => {
            // Stern: zwei gekreuzte Balken + Mitte
            img.fill_rect(rect(cx - 6, cy - 52, 12, 104), c);
            img.fill_rect(rect(cx - 52, cy - 6, 104, 12), c);
            circle(img, cx, cy, 20, c);
        }
        CardKind::Aura => {
            // Halo über einer Figur
            circle(img, cx, cy - 34, 26, c);
            circle(img, cx, cy - 34, 18, c.lightened(0.85));
            circle(img, cx, cy + 4, 14, c);
            img.fill_rect(rect(cx - 20, cy + 18, 40, 38), c);
        }
        CardKind::Battle => {
            // Schild: Fläche mit Spitze nach unten und Mittelstreifen
            img.fill_rect(rect(cx - 40, cy - 46, 80, 60), c);
            for k in 0..30 {
                img.fill_rect(rect(cx - 40 + k * 4 / 3, cy + 14 + k, 80 - k * 8 / 3, 1), c);
            }
            img.fill_rect(rect(cx - 5, cy - 46, 10, 88), c.lightened(0.6));
        }
        CardKind::Creature => {
            // zwei gekreuzte Klingen mit Griffen
            for k in -44..=44 {
                img.fill_rect(rect(cx + k - 5, cy + k - 5, 11, 11), c);
                img.fill_rect(rect(cx - k - 5, cy + k - 5, 11, 11), c);
            }
            for (gx, gy) in [(-44, 44), (44, 44)] {
                img.fill_rect(rect(cx + gx - 16, cy + gy - 16 + 6, 32, 8), c.darkened(0.3));
                circle(img, cx + gx + gx.signum() * 6, cy + gy + 12, 8, c);
            }
        }
        CardKind::Land => {}
    }
}

/// Vorderseite einer verdeckten Karte (CR 708): Rückseitenmuster, Titelleiste und – bei
/// Kreaturen – Wertefeld für die Werte der verdeckten Seite (meist 2/2).
fn face_down_face(def: &CardDef) -> Gd<Texture2D> {
    let mut img = back_image();
    let dark = Color::from_rgb(0.08, 0.08, 0.12);
    img.fill_rect(rect(20, 20, W - 40, 32), dark); // Titel
    img.fill_rect(rect(20, 208, W - 40, 22), dark); // Typzeile
    if def.stats.is_some() {
        img.fill_rect(rect(W - 82, 318, 64, 32), Color::from_rgb(0.12, 0.08, 0.06));
        img.fill_rect(rect(W - 80, 320, 60, 28), dark);
    }
    finish(img)
}

/// Gemeinsame Rückseite (einmal erzeugt, von allen Karten geteilt).
pub fn card_back() -> Gd<Texture2D> {
    super::shared::texture("card_back", || finish(back_image()))
}

/// Kartenhülle aus einer Bilddatei (z. B. aus NeoForges „Anpassen“): auf das Format der
/// Kartentextur gebracht, als Rückseite. `None`, wenn die Datei nicht lesbar ist.
pub fn sleeve_from_file(path: &str) -> Option<Gd<Texture2D>> {
    let mut img = Image::load_from_file(path)?;
    img.convert(Format::RGBA8);
    img.resize_ex(W, H).interpolation(godot::classes::image::Interpolation::LANCZOS).done();
    Some(finish(img))
}

/// Porträt aus einer Bilddatei für ein Medaillon: mittig quadratisch zugeschnitten, höchstens
/// 256 Pixel (eigene Fotos können riesig sein). `None`, wenn die Datei nicht lesbar ist.
pub fn portrait_from_file(path: &str) -> Option<Gd<Texture2D>> {
    let img = Image::load_from_file(path)?;
    let (w, h) = (img.get_width(), img.get_height());
    let side = w.min(h);
    if side <= 0 {
        return None;
    }
    let mut sq = img.get_region(rect((w - side) / 2, (h - side) / 2, side, side))?;
    sq.convert(Format::RGBA8);
    let size = side.min(256);
    sq.resize_ex(size, size).interpolation(godot::classes::image::Interpolation::LANCZOS).done();
    Some(finish(sq))
}

fn back_image() -> Gd<Image> {
    let mut img = Image::create_empty(W, H, false, Format::RGBA8).expect("Image");
    img.fill(Color::from_rgb(0.08, 0.08, 0.1));
    let colors = [
        Color::from_rgb(0.12, 0.16, 0.38),
        Color::from_rgb(0.75, 0.6, 0.25),
        Color::from_rgb(0.18, 0.24, 0.52),
        Color::from_rgb(0.75, 0.6, 0.25),
        Color::from_rgb(0.12, 0.16, 0.38),
    ];
    for (k, c) in colors.iter().enumerate() {
        let m = 10 + k as i32 * 18;
        img.fill_rect(rect(m, m, W - 2 * m, H - 2 * m), *c);
    }
    img
}

/// Größe der Kachel-Textur (Pixel): Breite wie das Kartenbild, Höhe im Seitenverhältnis der Kachel.
pub const TILE_W: i32 = 256;
pub const TILE_H: i32 = 215;
/// Kachel: Namensleiste (Mitte y), Wertekästchen (Mitte y, Breite, Höhe), runder Knopf links unten
/// (Mitte x, y, Radius) – Pixel der Kachel-Textur.
pub const TILE_NAME_Y: i32 = 20;
pub const TILE_STATS_Y: i32 = 192;
const TILE_STATS_W: i32 = 84;
const TILE_STATS_H: i32 = 30;
const TILE_ICON: (i32, i32, i32) = (26, 190, 21);

thread_local! {
    /// Kachel-Texturen je Aufbau (Rahmenfarbe, Bildausschnitt, Werte, Knopf).
    static TILES: RefCell<HashMap<String, Gd<Texture2D>>> = RefCell::new(HashMap::new());
}

/// Hat die Karte eine Fähigkeit mit {T} in den Kosten (Knopf ↷), sonst ◎?
fn has_tap_ability(def: &CardDef) -> bool {
    def.text.lines().any(|l| l.split(':').next().is_some_and(|cost| l.contains(':') && cost.contains("{T}")))
}

/// Bildfeld eines ganzen Kartenbilds (links, oben, rechts, unten als Anteile) – für Kacheln von
/// Karten ohne eigenen Bildausschnitt, z. B. Spielsteine.
const FULL_ART: (f32, f32, f32, f32) = (0.08, 0.115, 0.92, 0.555);

/// Kachel einer Karte auf dem Spielfeld wie in `Cratzuren icons.png`: Rahmen in Kartenfarbe,
/// dunkle Namensleiste oben, der Bildausschnitt (Scryfall `art_crop`, sonst das Bildfeld des ganzen Kartenbilds, z. B. bei
/// Spielsteinen) in der Mitte, unten eine
/// Leiste mit hellem, abgerundetem Wertekästchen (Stärke/Widerstand, Verteidigung; Loyalität hat
/// einen eigenen Schild an der Karte;
/// Text per Label3D) und links unten ein runder schwarzer Knopf mit weißem Symbol (↷ bei einer
/// Tapp-Fähigkeit, sonst ◎). Name und Werte kommen per Label3D dazu. None ohne Bild.
pub fn tile_face(def: &CardDef) -> Option<Gd<Texture2D>> {
    // ohne Bildausschnitt (z. B. Spielsteine): der Bildbereich aus dem ganzen Kartenbild
    let from_full = def.crop.is_empty();
    let source = if from_full { &def.image } else { &def.crop };
    if def.face_down || source.is_empty() || CLOSED.with(|c| c.get()) {
        return None;
    }
    // Planeswalker: Loyalität im eigenen Schild der Karte (Card3D), kein Wertekästchen
    let stats = matches!(def.kind, CardKind::Creature | CardKind::Battle) || def.stats.is_some();
    let tap = has_tap_ability(def);
    let base = def.frame_color();
    let key = format!("{}|{}|{}|{}", source, base.to_html(), stats, tap);
    if let Some(t) = TILES.with(|m| m.borrow().get(&key).cloned()) {
        return Some(t);
    }
    let mut art = Image::load_from_file(source)?;
    art.convert(Format::RGBA8);
    if from_full {
        // Bildfeld eines Scryfall-Kartenbilds (normale Karte und Spielstein): unter der Namenszeile,
        // über der Typzeile bzw. dem Textfeld – nur der Bildteil, nicht die halbe Karte
        let (iw, ih) = (art.get_width() as f32, art.get_height() as f32);
        let r = Rect2i::new(
            Vector2i::new((iw * FULL_ART.0) as i32, (ih * FULL_ART.1) as i32),
            Vector2i::new((iw * (FULL_ART.2 - FULL_ART.0)) as i32, (ih * (FULL_ART.3 - FULL_ART.1)) as i32),
        );
        art = art.get_region(r)?;
    }
    let (w, h) = (TILE_W, TILE_H);
    let frame = base.darkened(0.25);
    let dark = Color::from_rgb(0.08, 0.08, 0.09);
    let mut img = Image::create_empty(w, h, false, Format::RGBA8)?;
    img.fill(dark);
    img.fill_rect(rect(4, 4, w - 8, h - 8), frame);
    // Namensleiste
    img.fill_rect(rect(10, 9, w - 20, 22), base.lightened(0.55));
    img.fill_rect(rect(10, 30, w - 20, 2), dark);
    // Bild: auf Feldbreite skaliert, mittig beschnitten
    let (fx, fy, fw, fh) = (10, 34, w - 20, h - 34 - 34);
    let (aw, ah) = (art.get_width().max(1) as f32, art.get_height().max(1) as f32);
    let scale = (fw as f32 / aw).max(fh as f32 / ah);
    let (nw, nh) = ((aw * scale).ceil() as i32, (ah * scale).ceil() as i32);
    art.resize_ex(nw, nh).interpolation(godot::classes::image::Interpolation::LANCZOS).done();
    img.blit_rect(&art, rect((nw - fw) / 2, (nh - fh) / 2, fw, fh), Vector2i::new(fx, fy));
    // untere Leiste
    img.fill_rect(rect(10, h - 32, w - 20, 22), base.lightened(0.35));
    if stats {
        // helles, abgerundetes Wertekästchen
        let (cx, cy) = (w / 2, TILE_STATS_Y);
        let (bw, bh) = (TILE_STATS_W, TILE_STATS_H);
        img.fill_rect(rect(cx - bw / 2 - 2, cy - bh / 2 - 2, bw + 4, bh + 4), Color::from_rgb(0.35, 0.36, 0.38));
        img.fill_rect(rect(cx - bw / 2 + 4, cy - bh / 2, bw - 8, bh), Color::from_rgb(0.9, 0.9, 0.88));
        circle(&mut img, cx - bw / 2 + 4, cy, bh / 2, Color::from_rgb(0.9, 0.9, 0.88));
        circle(&mut img, cx + bw / 2 - 4, cy, bh / 2, Color::from_rgb(0.9, 0.9, 0.88));
    }
    // runder Knopf links unten: schwarz mit weißem Rand, Symbol ↷ bzw. ◎
    let (ix, iy, ir) = TILE_ICON;
    circle(&mut img, ix, iy, ir + 2, Color::from_rgb(0.95, 0.95, 0.95));
    circle(&mut img, ix, iy, ir - 1, dark);
    if tap {
        if let Some(mut sym) = crate::game::mana_symbols::image("T") {
            // Tapp-Symbol aus dem Bogen (grauer Kreis, schwarzer Pfeil) → weißer Pfeil auf Schwarz
            let s = (ir as f32 * 1.6) as i32;
            sym.resize_ex(s, s).interpolation(godot::classes::image::Interpolation::LANCZOS).done();
            for y in 0..s {
                for x in 0..s {
                    let c = sym.get_pixel(x, y);
                    let ink = c.a * (1.0 - (c.r + c.g + c.b) / 3.0).clamp(0.0, 1.0);
                    if ink > 0.35 {
                        img.set_pixel(ix - s / 2 + x, iy - s / 2 + y, Color::from_rgb(0.97, 0.97, 0.97));
                    }
                }
            }
        }
    } else {
        circle(&mut img, ix, iy, ir - 6, Color::from_rgb(0.95, 0.95, 0.95));
        circle(&mut img, ix, iy, ir - 9, dark);
        circle(&mut img, ix, iy, ir - 13, Color::from_rgb(0.95, 0.95, 0.95));
    }
    let tex = finish(img);
    TILES.with(|m| m.borrow_mut().insert(key, tex.clone()));
    Some(tex)
}