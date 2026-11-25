//! Kartendaten für Kartenbild und Anordnung, gelesen aus den Kartendaten der Engine (`state`).
//! Keine Regeln: Werte, Text und Typ kommen fertig von der Engine; hier wird nur bestimmt, wie
//! die Karte aussieht (Rahmenfarbe, Typsymbol, Wertefeld) und wo sie liegt.

use godot::prelude::*;

use super::card_text;
use super::mana::{Cost, ManaType};

/// Hauptkategorie einer Karte (aus der Typzeile, CR 205.2). Bestimmt Typsymbol, Wertefeld und
/// Platz auf dem Tisch (Länder in der Länder-Reihe, Kreaturen in der Mitte, Planeswalker und
/// Schlachten links, alles andere rechts).
#[derive(GodotConvert, Var, Export, Clone, Copy, PartialEq, Eq, Hash, Debug, Default)]
#[godot(via = i64)]
pub enum CardKind {
    #[default]
    Creature,
    Land,
    Instant,
    Sorcery,
    Planeswalker,
    /// Artefakt – Ausrüstung.
    Equipment,
    Artifact,
    Enchantment,
    /// Verzauberung – Aura.
    Aura,
    /// Schlacht (CR 310).
    Battle,
}

impl CardKind {
    /// Kategorie aus einer englischen Typzeile (z. B. „Artifact Creature — Golem“). Reihenfolge
    /// nach Vorrang: Land vor Kreatur (Artefaktländer), Kreatur vor Artefakt/Verzauberung.
    pub fn from_type_line(ty: &str) -> CardKind {
        let has = |w: &str| ty.split(|c: char| !c.is_alphanumeric()).any(|t| t == w);
        if has("Land") {
            CardKind::Land
        } else if has("Creature") {
            CardKind::Creature
        } else if has("Planeswalker") {
            CardKind::Planeswalker
        } else if has("Battle") {
            CardKind::Battle
        } else if has("Instant") {
            CardKind::Instant
        } else if has("Sorcery") {
            CardKind::Sorcery
        } else if has("Aura") {
            CardKind::Aura
        } else if has("Equipment") {
            CardKind::Equipment
        } else if has("Enchantment") {
            CardKind::Enchantment
        } else {
            CardKind::Artifact
        }
    }
}

/// Alles, was das Kartenbild braucht.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct CardDef {
    pub name: String,
    /// Typzeile wie von der Engine (Gedankenstrich als „—“).
    pub type_line: String,
    pub kind: CardKind,
    /// Manakosten für die Kostensymbole.
    pub cost: Cost,
    /// Manakosten in der Schreibweise der Engine („1 U“, „{X}{R}“), für die Kostensymbole über
    /// den Handkarten (`table::cost_symbols`; behält Hybrid, X und phyrexianische Symbole).
    pub cost_text: String,
    /// Pfad des Kartenbilds (JPG von Scryfall im Forge-Bildordner, leer = keins): ersetzt den
    /// gezeichneten Platzhalter (`table::placeholder::card_face`).
    pub image: String,
    /// Nur bei Karten ohne Druck mit normalem Rahmen: Pfad des Bildausschnitts (Scryfall
    /// `art_crop`); Bühnenkopien mit eigenem Text zeigen ihn im gezeichneten Rahmen.
    pub art: String,
    /// Bildausschnitt (Scryfall `art_crop`) für die Kachel auf dem Spielfeld, leer = keiner.
    pub crop: String,
    /// Regeltext wurde für eine Bühne ersetzt (ein Modus, ein Ersatzeffekt): auf dem echten
    /// Kartenbild steht dann nur dieser Text, auf einem Feld über dem gedruckten Textfeld.
    pub custom_text: bool,
    /// Kreatur ist noch nicht einsatzbereit (CR 302.6, „summoning sick“).
    pub sick: bool,
    /// Kreatur greift im laufenden Kampf an.
    pub attacking: bool,
    /// Farben der Karte (CR 105), leer = farblos.
    pub colors: Vec<ManaType>,
    /// Regeltext (Symbole wie `{T}` noch nicht ersetzt).
    pub text: String,
    /// Aktuelle und aufgedruckte Stärke/Widerstand (nur Kreaturen).
    pub stats: Option<(i32, i32)>,
    pub base_stats: Option<(i32, i32)>,
    /// Loyalität (Planeswalker) bzw. Verteidigung (Schlachten).
    pub loyalty: Option<i32>,
    pub defense: Option<i32>,
    /// Erlittener Schaden in diesem Zug.
    pub damage: i32,
    /// Verdeckt (Morph, Manifest …, CR 708): Bild mit Rückseitenmuster, Werte der verdeckten Seite.
    pub face_down: bool,
    /// Angezeigte Seite doppelseitiger Karten (0 = Vorderseite, 1 = Rückseite / umgedreht).
    pub face: u8,
    /// Nur Länder: Manafarbe für Rahmen und Spalte der Länder-Reihe (aus dem Grundlandtyp bzw.
    /// der einzigen Farbe; sonst farblos).
    pub produces: ManaType,
    /// Dungeon (CR 309): Räume und aktueller Raum – die Karte zeigt dann die gezeichnete
    /// Dungeon-Karte (`table::dungeon_view`).
    pub dungeon: Option<crate::game::dungeon::Dungeon>,
    /// Schlüsselwörter der Karte, wie die Engine sie nennt („Flying“, „First Strike“, „Ward 2“ …):
    /// Symbole unten an der Kreaturen-Kachel ([`keyword_icons`]), fliegende Kreaturen schweben.
    pub keywords: Vec<String>,
    /// Ausgephast (CR 702.26): bleibt halb durchsichtig mit Phasing-Symbol an seinem Platz.
    pub phased_out: bool,
    /// Kapitel-Fähigkeit einer Saga auf dem Stack (CR 714.2): Nummer des Kapitels, 0 = keine;
    /// das Kartenbild trägt dann eine Kapitelmarke mit römischer Ziffer.
    pub chapter: u32,
}

impl CardDef {
    /// Karte ohne Werte (z. B. verdeckt: Name „?“).
    pub fn named(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Self::default() }
    }

    /// Liest die Kartendaten des Protokolls: `name`, `type`, `cost`, `colors`, `text`, `power`,
    /// `toughness`, `base_power`, `base_toughness`, `loyalty`, `defense`, `damage`, `face_down`,
    /// `face` (fehlende Felder = leer).
    pub fn from_state(card: &VarDictionary) -> Self {
        let s = |k: &str| card.get(k).filter(|v| !v.is_nil()).map(|v| v.to_string()).unwrap_or_default();
        let i = |k: &str| card.get(k).and_then(|v| v.try_to::<i64>().ok()).map(|v| v as i32);
        let pair = |a: &str, b: &str| i(a).zip(i(b));
        let type_line = s("type").replace(" - ", " — ");
        let kind = CardKind::from_type_line(&type_line);
        let colors: Vec<ManaType> = s("colors").chars().filter_map(ManaType::from_wubrg).filter(|t| t.is_colored()).collect();
        let produces = if kind == CardKind::Land {
            [
                ("Plains", ManaType::White),
                ("Island", ManaType::Blue),
                ("Swamp", ManaType::Black),
                ("Mountain", ManaType::Red),
                ("Forest", ManaType::Green),
            ]
            .into_iter()
            .find(|(n, _)| type_line.contains(n))
            .map(|(_, t)| t)
            .or_else(|| (colors.len() == 1).then(|| colors[0]))
            .unwrap_or(ManaType::Colorless)
        } else {
            ManaType::Colorless
        };
        let stats = pair("power", "toughness");
        let face_down = card.get("face_down").and_then(|v| v.try_to::<bool>().ok()).unwrap_or(false);
        let name = match s("name") {
            n if n.is_empty() && face_down => tr!("Verdeckt", "Face-down").to_string(),
            n => n,
        };
        Self {
            name,
            face_down,
            face: i("face").unwrap_or(0).clamp(0, 1) as u8,
            kind,
            cost: parse_cost(&s("cost")),
            cost_text: s("cost"),
            // „nur eigene Grafik“ (`crate::assets`): keine Kartenbilder, nur eigene Rahmen
            image: if crate::assets::protected_enabled() { s("image") } else { String::new() },
            art: if crate::assets::protected_enabled() { s("art") } else { String::new() },
            crop: if crate::assets::protected_enabled() { s("crop") } else { String::new() },
            // Stack: nur die gewählten Modi stehen im Text (auf dem echten Bild auf einem Feld)
            custom_text: card.get("chosen_modes").is_some_and(|v| v.booleanize()),
            sick: card.get("sick").is_some_and(|v| v.booleanize()),
            attacking: card.get("attacking").is_some_and(|v| v.booleanize()),
            colors,
            text: s("text"),
            base_stats: pair("base_power", "base_toughness").or(stats),
            stats,
            loyalty: i("loyalty"),
            defense: i("defense"),
            damage: i("damage").unwrap_or(0),
            produces,
            type_line,
            dungeon: crate::game::dungeon::Dungeon::from_card(card),
            keywords: card
                .get("keywords")
                .and_then(|v| v.try_to::<VarArray>().ok())
                .map(|a| {
                    a.iter_shared()
                        .filter_map(|k| k.try_to::<VarDictionary>().ok())
                        .filter_map(|k| k.get("title").map(|t| t.to_string()))
                        .collect()
                })
                .unwrap_or_default(),
            phased_out: card.get("phased_out").is_some_and(|v| v.booleanize()),
            chapter: i("chapter").unwrap_or(0).clamp(0, 20) as u32,
        }
    }

    /// Hat die Karte Flugfähigkeit (CR 702.9)? Fliegende Kreaturen schweben über dem Spielfeld.
    pub fn has_flying(&self) -> bool {
        keyword_icons(&self.keywords).contains(&KeywordIcon::Flying)
    }

    /// Rahmenfarbe: Länder nach ihrer Manafarbe, sonst die Kartenfarbe (mehrfarbig = Gold,
    /// farblos = Grau).
    pub fn frame_color(&self) -> Color {
        if self.kind == CardKind::Land {
            return self.produces.color();
        }
        match self.colors.as_slice() {
            [] => ManaType::Colorless.color(),
            [one] => one.color(),
            _ => Color::from_rgb(0.85, 0.7, 0.3),
        }
    }

    /// Text und Farbe des Wertefelds: Kreatur „S/W“ (grün = über, rot = unter dem Aufdruck oder
    /// beschädigt), Planeswalker Loyalität, Schlacht Verteidigung; sonst leer.
    pub fn stats_text(&self) -> (String, Color) {
        const NORMAL: Color = Color::WHITE;
        const UP: Color = Color::from_rgb(0.6, 1.0, 0.5);
        const DOWN: Color = Color::from_rgb(1.0, 0.45, 0.4);
        if let Some((p, t)) = self.stats {
            let (bp, bt) = self.base_stats.unwrap_or((p, t));
            let color = if self.damage > 0 || p < bp || t < bt {
                DOWN
            } else if p > bp || t > bt {
                UP
            } else {
                NORMAL
            };
            return (format!("{p}/{t}"), color);
        }
        match (self.loyalty, self.defense) {
            (Some(l), _) => (l.to_string(), NORMAL),
            (None, Some(d)) => (d.to_string(), NORMAL),
            _ => (String::new(), NORMAL),
        }
    }

    /// Regeltext zum Anzeigen (Symbole ersetzt).
    pub fn display_text(&self) -> String {
        card_text::with_symbols(&self.text)
    }
}

/// Manakosten aus der Schreibweise der Engine (z. B. „2 R R“, „{1}{G}“, „X U“) → [`Cost`].
/// Symbole ohne festen Wert (X, hybrid, phyrexianisch …) werden ignoriert.
pub fn parse_cost(text: &str) -> Cost {
    let mut cost = Cost::default();
    for token in text.split(|c: char| c.is_whitespace() || c == '{' || c == '}').filter(|t| !t.is_empty()) {
        if let Ok(n) = token.parse::<u32>() {
            cost.generic += n;
            continue;
        }
        if token.len() != 1 {
            continue; // hybrid / phyrexianisch
        }
        if let Some(t) = token.chars().next().and_then(ManaType::from_wubrg) {
            cost.specific.add(t, 1);
        }
    }
    cost
}

/// Kosten in Engine-Schreibweise zusammenzählen, nur zur Anzeige (z. B. Spree: Grundkosten plus
/// gewählte Modus-Kosten, „{W}“ + „{1}“ + „{3}{W}{W}“ → „{4}{W}{W}{W}“): Zahlen werden addiert,
/// alle anderen Symbole (Farben, X, Hybrid, phyrexianisch …) bleiben erhalten, in WUBRG-Reihenfolge.
pub fn add_costs(parts: &[&str]) -> String {
    let mut generic = 0u32;
    let mut has_generic = false;
    let mut symbols: Vec<String> = Vec::new();
    for part in parts {
        for token in part.split(|c: char| c.is_whitespace() || c == '{' || c == '}').filter(|t| !t.is_empty()) {
            if let Ok(n) = token.parse::<u32>() {
                generic += n;
                has_generic = true;
            } else {
                symbols.push(token.to_string());
            }
        }
    }
    let rank = |t: &str| "WUBRG".find(t).unwrap_or(5);
    symbols.sort_by_key(|t| if t == "X" { 0 } else { 1 + rank(t) });
    let mut out = String::new();
    if symbols.first().map(String::as_str) == Some("X") {
        // X zuerst wie auf Karten („{X}{2}{R}“)
        while symbols.first().map(String::as_str) == Some("X") {
            out.push_str("{X}");
            symbols.remove(0);
        }
    }
    if has_generic && (generic > 0 || symbols.is_empty()) {
        out.push_str(&format!("{{{generic}}}"));
    }
    for t in symbols {
        out.push_str(&format!("{{{t}}}"));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn adds_costs_for_display() {
        assert_eq!(add_costs(&["{W}", "{1}", "{3}{W}{W}"]), "{4}{W}{W}{W}");
        assert_eq!(add_costs(&["{U}"]), "{U}");
        assert_eq!(add_costs(&["{1}{U}", "{2}"]), "{3}{U}");
        assert_eq!(add_costs(&["{X}{R}", "{1}"]), "{X}{1}{R}");
        assert_eq!(add_costs(&["{0}"]), "{0}");
        assert_eq!(add_costs(&[]), "");
    }

    #[test]
    fn parses_costs() {
        let c = parse_cost("2 R R");
        assert_eq!(c.generic, 2);
        assert_eq!(c.specific.get(ManaType::Red), 2);
        let c = parse_cost("{1}{G}");
        assert_eq!(c.generic, 1);
        assert_eq!(c.specific.get(ManaType::Green), 1);
        assert_eq!(parse_cost("X U").specific.get(ManaType::Blue), 1);
        assert_eq!(parse_cost("{R/P}").total(), 0);
    }

    #[test]
    fn kinds_from_type_lines() {
        assert_eq!(CardKind::from_type_line("Artifact Creature — Golem"), CardKind::Creature);
        assert_eq!(CardKind::from_type_line("Artifact Land"), CardKind::Land);
        assert_eq!(CardKind::from_type_line("Legendary Planeswalker — Chandra"), CardKind::Planeswalker);
        assert_eq!(CardKind::from_type_line("Enchantment — Aura"), CardKind::Aura);
        assert_eq!(CardKind::from_type_line("Artifact — Equipment"), CardKind::Equipment);
        assert_eq!(CardKind::from_type_line("Kindred Instant — Goblin"), CardKind::Instant);
    }
}


/// Schlüsselwort mit eigenem Symbol unten an der Kreaturen-Kachel (wie in Arena; die Symbole sind
/// eigene Zeichnungen, `table::cost_symbols::draw_keyword`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KeywordIcon {
    Flying,
    FirstStrike,
    DoubleStrike,
    Deathtouch,
    Menace,
    Trample,
    Vigilance,
    Lifelink,
    Reach,
    Haste,
    Hexproof,
    Ward,
    Indestructible,
    Defender,
}

impl KeywordIcon {
    /// Kennung im Symbol-Token (`&flying` …).
    pub fn id(self) -> &'static str {
        match self {
            Self::Flying => "flying",
            Self::FirstStrike => "first_strike",
            Self::DoubleStrike => "double_strike",
            Self::Deathtouch => "deathtouch",
            Self::Menace => "menace",
            Self::Trample => "trample",
            Self::Vigilance => "vigilance",
            Self::Lifelink => "lifelink",
            Self::Reach => "reach",
            Self::Haste => "haste",
            Self::Hexproof => "hexproof",
            Self::Ward => "ward",
            Self::Indestructible => "indestructible",
            Self::Defender => "defender",
        }
    }
}

/// Symbole zu den Schlüsselwörtern `keywords` (Namen der Engine), in fester Reihenfolge und ohne
/// Dubletten. Doppelschlag schließt Erstschlag ein (CR 702.4b), dann nur das Doppelschlag-Symbol.
pub fn keyword_icons(keywords: &[String]) -> Vec<KeywordIcon> {
    use KeywordIcon::*;
    const ORDER: [(KeywordIcon, &str); 14] = [
        (Flying, "flying"),
        (DoubleStrike, "double strike"),
        (FirstStrike, "first strike"),
        (Deathtouch, "deathtouch"),
        (Menace, "menace"),
        (Trample, "trample"),
        (Vigilance, "vigilance"),
        (Lifelink, "lifelink"),
        (Reach, "reach"),
        (Haste, "haste"),
        (Hexproof, "hexproof"),
        (Ward, "ward"),
        (Indestructible, "indestructible"),
        (Defender, "defender"),
    ];
    let lower: Vec<String> = keywords.iter().map(|k| k.trim().to_lowercase()).collect();
    let has = |name: &str| lower.iter().any(|k| k == name || k.starts_with(&format!("{name} ")) || k.starts_with(&format!("{name}:")));
    let mut out: Vec<KeywordIcon> = ORDER.iter().filter(|(_, n)| has(n)).map(|(i, _)| *i).collect();
    if out.contains(&DoubleStrike) {
        out.retain(|i| *i != FirstStrike);
    }
    out
}

#[cfg(test)]
mod keyword_tests {
    use super::*;

    fn kw(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn icons_in_fixed_order_without_duplicates() {
        let icons = keyword_icons(&kw(&["Menace", "Flying", "Deathtouch", "Flying"]));
        assert_eq!(icons, vec![KeywordIcon::Flying, KeywordIcon::Deathtouch, KeywordIcon::Menace]);
    }

    #[test]
    fn double_strike_replaces_first_strike() {
        let icons = keyword_icons(&kw(&["First Strike", "Double Strike"]));
        assert_eq!(icons, vec![KeywordIcon::DoubleStrike]);
    }

    #[test]
    fn keywords_with_parameters() {
        let icons = keyword_icons(&kw(&["Ward 2", "Hexproof from black", "Trample over planeswalkers", "Flash"]));
        assert_eq!(icons, vec![KeywordIcon::Trample, KeywordIcon::Hexproof, KeywordIcon::Ward]);
    }
}
