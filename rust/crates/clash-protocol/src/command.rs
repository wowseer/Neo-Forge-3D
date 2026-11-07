//! Befehle an die Brücke (eine JSON-Zeile je Befehl, Feld `cmd`). Siehe
//! `docs/Forge-Protocol.md`, Abschnitt „Befehle“.

use serde_json::{json, Map, Value};

use crate::{Choice, PlayerId};

/// Verbindungstest; Antwort `{"type":"pong","nonce":…}`.
pub fn ping(nonce: i64) -> Value {
    json!({ "cmd": "ping", "nonce": nonce })
}

/// Antwort `choice` auf die Anfrage `request_id`.
pub fn answer(request_id: i64, choice: &Choice) -> Value {
    answer_json(request_id, choice.to_json())
}

/// Antwort in JSON-Form (z. B. aus einer Skriptsprache) auf die Anfrage `request_id`.
pub fn answer_json(request_id: i64, choice: Value) -> Value {
    json!({ "cmd": "answer", "id": request_id, "choice": choice })
}

/// Spielzustand anfordern; `viewer` = Sitzplatz, dessen Handkarten mitkommen (`None` = keiner).
pub fn request_state(viewer: Option<PlayerId>) -> Value {
    json!({ "cmd": "state", "viewer": viewer.map_or(-1, i64::from) })
}

/// Lage des Tisch-Fensters melden (eingebettet: Fensterübergabe, der Gastgeber legt sein Fenster nach
/// der Partie dorthin). Pixel des Bildschirms, Inhaltsbereich ohne Rahmen; `mode` =
/// `"windowed"`, `"maximized"` oder `"fullscreen"`.
pub fn window(x: i32, y: i32, width: i32, height: i32, mode: &str) -> Value {
    json!({ "cmd": "window", "x": x, "y": y, "width": width, "height": height, "mode": mode })
}

/// Brücke beenden (eigenständiger Betrieb; eingebettet ignoriert).
pub fn quit() -> Value {
    json!({ "cmd": "quit" })
}

/// Partie starten (nur eigenständiger Betrieb; eingebettet startet der Gastgeber).
#[derive(Debug, Clone, PartialEq)]
pub struct StartMatch {
    /// Je Sitzplatz ein Deck: Pfad zu einer Forge-`.dck`-Datei (absolut oder relativ zum
    /// Forge-Ordner, dem Arbeitsverzeichnis der Brücke) oder Name eines Decks aus Forges
    /// Constructed-Sammlung (mindestens zwei).
    pub decks: Vec<String>,
    /// Sitzplätze, die die Oberfläche steuert (Rest spielt die Forge-KI).
    pub remote: Vec<PlayerId>,
    /// Spielernamen (darf kürzer sein als `decks`).
    pub names: Vec<String>,
    /// Zufalls-Startwert (`None` = zufällig).
    pub seed: Option<i64>,
    /// Tempo der Forge-KI (Faktor der Pausen nach ihren Aktionen; 0 = ohne Pausen).
    pub ai_pace: f64,
    /// Forge-Spieltyp, z. B. `"Constructed"` (Standard) oder `"Commander"`.
    pub game_type: Option<String>,
}

impl Default for StartMatch {
    fn default() -> Self {
        Self { decks: Vec::new(), remote: vec![0], names: Vec::new(), seed: None, ai_pace: 1.0, game_type: None }
    }
}

impl StartMatch {
    /// JSON-Form des Befehls `start_match`.
    pub fn to_json(&self) -> Value {
        let mut m = Map::new();
        m.insert("cmd".into(), "start_match".into());
        m.insert("decks".into(), json!(self.decks));
        let remote: Vec<bool> = (0..self.decks.len()).map(|i| self.remote.iter().any(|r| usize::from(*r) == i)).collect();
        m.insert("remote".into(), json!(remote));
        m.insert("names".into(), json!(self.names));
        if let Some(s) = self.seed {
            m.insert("seed".into(), s.into());
        }
        m.insert("ai_pace".into(), self.ai_pace.into());
        if let Some(t) = &self.game_type {
            m.insert("game_type".into(), t.clone().into());
        }
        Value::Object(m)
    }
}

/// Kartendaten (Name, Typ, Kosten, Text, Stärke/Widerstand, Bild) zu Kartennamen anfordern –
/// Antwort `{"type":"card_info","cards":[…]}`; unbekannte Namen fehlen in der Antwort.
pub fn card_info(names: &[String]) -> Value {
    json!({ "cmd": "card_info", "names": names })
}

/// Debug: alle Kartennamen anfordern (Antwort `debug_card_names`).
pub fn debug_card_names() -> Value {
    json!({ "cmd": "debug_card_names" })
}

/// Debug: Karte `name` neu erzeugen und Sitzplatz `seat` auf die Hand legen.
pub fn debug_add_card(seat: PlayerId, name: &str) -> Value {
    json!({ "cmd": "debug_add_card", "seat": seat, "name": name })
}

/// Debug: unendliches Mana für Sitzplatz `seat` an/aus.
pub fn debug_infinite_mana(seat: PlayerId, on: bool) -> Value {
    json!({ "cmd": "debug_infinite_mana", "seat": seat, "on": on })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn window_has_geometry_and_mode() {
        let w = window(10, 20, 1920, 1080, "maximized");
        assert_eq!(w, json!({"cmd": "window", "x": 10, "y": 20, "width": 1920, "height": 1080, "mode": "maximized"}));
    }

    #[test]
    fn start_match_marks_remote_seats() {
        let m = StartMatch { decks: vec!["a.dck".into(), "b.dck".into()], ..Default::default() }.to_json();
        assert_eq!(m["remote"], json!([true, false]));
        assert_eq!(m["cmd"], "start_match");
        assert!(m.get("seed").is_none());
    }
}
