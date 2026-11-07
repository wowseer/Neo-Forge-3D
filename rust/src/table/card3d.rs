//! Card3D: eine einzelne Karte auf dem 3D-Tisch.
//!
//! Aufbau (wird in `ready()` per Code erzeugt):
//! ```text
//! Card3D (Area3D, Kollisions-Layer CARD_LAYER)   <- wird per Tween bewegt
//! ├── CollisionShape3D (BoxShape3D)                <- Raycast-Ziel, bleibt ruhig
//! └── Visual (Node3D)                              <- Tilt / Hover-Zoom / Drag-Schwung
//!     ├── Body  (BoxMesh, dünner Kartenkörper)
//!     ├── Face  (PlaneMesh, +Y, Vorderseiten-Textur)
//!     ├── Back  (PlaneMesh, -Y, Rückseiten-Textur)
//!     ├── Marker (PlaneMesh unter der Karte, Lichtsaum: kann handeln / Angriff / Block / Auswahl)
//!     └── TapIcon (PlaneMesh auf der Vorderseite, graues Tapp-Symbol; getappt, aber nicht angegriffen)
//! ```
//! **Getappt:** Die Zone stellt die Karte leicht schräg (`zone::TAP_ANGLE_DEGREES`); sie wird
//! ausgegraut (Vorderseite und Beschriftung abgedunkelt) und zeigt das graue Tapp-Symbol.
//! **Hat angegriffen:** ebenfalls schräg und ausgegraut, aber **ohne** Tapp-Symbol.
//!
//! Die Karte liegt in ihrem lokalen Raum flach in der XZ-Ebene, Vorderseite zeigt
//! nach +Y, die Oberkante der Karte zeigt nach -Z.
//!
//! **Hover-Zoom:** Ist die Vorderseite sichtbar, schwebt nur das `Visual` zur
//! Kamera, dreht sich aufrecht zum Betrachter und wird so groß, dass sie
//! `zoom_screen_height` der Bildschirmhöhe einnimmt – immer vollständig im Bild.
//! Die Kollisionsform bleibt am Tisch, das Hovern bleibt dadurch stabil.

use godot::classes::base_material_3d::{ShadingMode, TextureFilter, TextureParam};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{
    Area3D, BoxMesh, BoxShape3D, CollisionShape3D, IArea3D, MeshInstance3D, PlaneMesh,
    ShaderMaterial, StandardMaterial3D, Texture2D, Tween,
};
use godot::prelude::*;

use godot::classes::label_3d::DrawFlags;
use godot::classes::Label3D;
use godot::global::HorizontalAlignment;

use crate::game::card_text;
use crate::game::cards::{CardDef, CardKind};
use crate::game::mana::ManaType;
use super::placeholder;
use super::layers::CARD_LAYER;
use super::shader_noise::PERLIN_FBM;
use super::cost_symbols;
use super::zone::Zone;

pub const CARD_WIDTH: f32 = 0.63;
/// Kostensymbole über Handkarten: Durchmesser und Abstand vom rechten Rand (Welteinheiten).
const HAND_COST_SIZE: f32 = 0.15;
const HAND_COST_INSET: f32 = 0.03;
/// Marken-Münzen: Durchmesser (Welteinheiten) und Dauer von Aufspringen/Leuchten (Sekunden).
const COIN_SIZE: f32 = 0.2;
const COIN_POP_TIME: f32 = 0.5;
/// Loyalitäts-Schild: Höhe (m) und Dauer des Aufblitzens bei einer Änderung (Sekunden).
const SHIELD_SIZE: f32 = 0.27;
const SHIELD_FLASH_TIME: f32 = 0.6;
pub const CARD_HEIGHT: f32 = 0.88;
pub const CARD_THICKNESS: f32 = 0.006;
/// Kachel-Form auf dem Spielfeld (wie im Video des Users): nur der obere Teil der Karte – Name
/// und Bild –, Stärke/Widerstand rechts unten. Anteil der Kartenhöhe und Höhe der Kachel.
pub const TILE_FRACTION: f32 = 0.6;
pub const TILE_HEIGHT: f32 = CARD_HEIGHT * TILE_FRACTION;

/// Farben des Kartenrahmens (Rahmen-Glow aus `Assets/Mana Glow Effekte`, linear wie in Blender):
/// **Blau** = Standard, die Karte ist spielbar bzw. wählbar; **Orange** = aktiviert (wird gezogen
/// oder gespielt, Fähigkeit aktiviert, als Angreifer/Blocker/Ziel gewählt, Land für Mana getappt).
pub const PLAYABLE_GLOW: Color = Color::from_rgb(0.02, 0.28, 1.0);
pub const ACTIVE_GLOW: Color = Color::from_rgb(1.0, 0.22, 0.0);
/// **Gelb** = in einem erklärten Block (Video „Blocken“): Angreifer und Blocker, sobald die
/// Blocker feststehen (CR 509.1), bis zum Ende des Kampfes.
pub const BLOCKED_GLOW: Color = Color::from_rgb(1.0, 0.62, 0.02);
/// Helligkeit des Rahmens: spielbar (ruhig) bzw. aktiviert (kräftiger).
const READY_STRENGTH: f32 = 1.0;
/// Aufleuchten beim Aktivieren ([`Card3D::flash`]; Blender: Einschlag Frame 30 → 80):
/// Verstärkung über die Zeit (Sekunden, Faktor), danach orange bis zum Ende.
const FLASH_KEYS: [(f32, f32); 5] = [(0.0, 1.0), (0.07, 4.6), (0.27, 2.7), (0.67, 1.7), (1.6, 1.0)];
/// Angreifer: rückt so weit nach vorn (zum Gegner, lokal −Z), höchstens [`ADVANCE_MAX_WORLD`]
/// Welteinheiten – gut ein Drittel der Lücke zwischen den Kreaturenreihen beider Seiten (0,8,
/// `zone.rs` `FRONT_GAP`), damit zu den Karten gegenüber deutlich Abstand bleibt.
const ADVANCE: f32 = 0.22;
const ADVANCE_MAX_WORLD: f32 = 0.3;
/// Schlag im Kampf (wie im Video „Blocken“): Ausholen zum Ziel, Rückweg (Sekunden), Höhe über dem
/// Ziel beim Treffer (die Karte landet auf ihm statt hindurch) und Drehung in der Tischebene
/// während des Flugs (Grad).
pub const LUNGE_OUT: f32 = 0.3;
const LUNGE_BACK: f32 = 0.38;
const LUNGE_LIFT: f32 = 0.6;
const LUNGE_TURN_DEGREES: f32 = 16.0;
/// Nach dem Treffer bleiben Angreifer und Getroffener so lange liegen (Sekunden), bevor sie
/// weiterziehen (z. B. zum Friedhof) – sonst überdeckte die Fahrt den Schlag.
pub const LUNGE_HOLD: f32 = 0.15;
/// Rahmenfarbe wechselt weich (Faktor je Sekunde, ≈ 0,25 s), z. B. Blau → Orange beim Zuweisen
/// eines Blockers.
const GLOW_FADE_SPEED: f32 = 14.0;
/// Doppelseitige Karte umdrehen (ANIMATION.md): Dauer, Anheben in der Mitte (Cubic-In-Out).
const FLIP_DURATION: f32 = 0.35;
const FLIP_LIFT: f32 = 0.25;
const MARK_STRENGTH: f32 = 1.45;
/// Farbe, mit der getappte Karten (und Kreaturen, die angegriffen haben) ausgegraut werden.
const TAPPED_TINT: Color = Color::from_rgb(0.52, 0.52, 0.55);
/// Kantenlänge des Tapp-Symbols auf der Karte (Einheiten; Karte = 0,63 breit).
const TAP_ICON_SIZE: f32 = 0.34;
/// Wie weit die Aura über den Kartenrand hinausragt (Einheiten).
const GLOW_MARGIN: f32 = 0.16;

/// Größe der Angriffsspitze (Breite, Höhe) über der Oberkante der Karte.
const BANNER_SIZE: Vector2 = Vector2::new(0.36, 0.15);
/// Blockschweif ([method Card3D.set_block_trail]): Höhe über der Kante zum Angreifer (Einheiten),
/// Breite = Karte plus [`TRAIL_MARGIN`] je Seite.
const TRAIL_HEIGHT: f32 = 0.24;
const TRAIL_MARGIN: f32 = 0.05;
/// Ein-/Ausblenden des Blockschweifs und Übergang Orange → Gelb (Faktor je Sekunde, ≈ 0,25 s).
const TRAIL_FADE_SPEED: f32 = 12.0;

/// Angriffsspitze: Fünfeck als Dach (unten breit, oben Spitze), rot mit hellem Rand und Schein.
const BANNER_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_mix, cull_disabled, depth_draw_never, shadows_disabled;
float sd_roof(vec2 p) {
    // p: x −1…1, y 0 (unten) … 1 (Spitze)
    float side = abs(p.x) - 0.92;
    float roof = (p.y - 0.45) + abs(p.x) * 0.55 - 0.5;
    float bottom = -p.y + 0.02;
    return max(max(side, roof), bottom);
}
// 0 = rot (Angriff), 1 = gelb (geblockt, erklärter Block)
uniform float yellow = 0.0;
void fragment() {
    vec2 p = vec2(UV.x * 2.0 - 1.0, 1.0 - UV.y);
    float d = sd_roof(p);
    float fill = 1.0 - smoothstep(-0.01, 0.02, d);
    float rim = 1.0 - smoothstep(0.0, 0.06, abs(d + 0.05));
    float glow = exp(-max(d, 0.0) / 0.08) * 0.6;
    vec3 base = mix(vec3(0.75, 0.08, 0.04), vec3(1.0, 0.58, 0.02), yellow);
    vec3 light = mix(vec3(1.0, 0.55, 0.3), vec3(1.0, 0.95, 0.6), yellow);
    vec3 col = mix(base, light, rim * fill);
    ALBEDO = col;
    ALPHA = clamp(fill + glow * (1.0 - fill), 0.0, 1.0);
}
"#;

/// Blockschweif: flackernde Lichtzungen, die als Bogen aus der Kante des Blockers zum Angreifer
/// aufsteigen (in der Mitte am höchsten). `amount` blendet ein und aus, `yellow` wechselt von
/// Orange (geplanter Block) zu Gelb (erklärter Block, ruhiger). Additiv (vormultipliziert,
/// Deckkraft 0 – siehe `fx.rs`), unbeleuchtet.
const TRAIL_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_premul_alpha, cull_disabled, depth_draw_never, shadows_disabled, fog_disabled;
uniform float amount = 0.0;
uniform float yellow = 0.0;
uniform float seed = 0.0;
%NOISE%
void fragment() {
    float x = UV.x * 2.0 - 1.0;
    // h: 0 an der Kartenkante (+z des Quads), 1 am äußeren Ende (zum Angreifer)
    float h = 1.0 - UV.y;
    float arc = sqrt(max(1.0 - x * x, 0.0));
    float n = clamp(fbm(vec4(x * 2.2 + seed, h * 1.4 - TIME * 1.7, seed, TIME * 0.35), 3, 0.5), 0.0, 1.0);
    // geplant: lebhafte Zungen; erklärt: ruhiger, gleichmäßiger Bogen
    float flick = mix(0.35 + n * 1.0, 0.8 + n * 0.3, yellow);
    float reach = max(arc * flick * amount, 0.001);
    float body = (1.0 - smoothstep(reach * 0.35, reach, h)) * (1.0 - h * 0.6);
    float edge = exp(-h / 0.07) * arc;
    vec3 c = mix(vec3(1.0, 0.22, 0.0), vec3(1.0, 0.62, 0.02), yellow);
    vec3 col = mix(c, vec3(1.0, 0.93, 0.7), clamp(edge * 0.7 + body * 0.15, 0.0, 1.0));
    float side = 1.0 - smoothstep(0.85, 1.0, abs(x));
    ALBEDO = col * (body * 1.5 + edge * 0.9) * amount * side;
    ALPHA = 0.0;
}
"#;

/// Rahmen-Glow (Nachbau von „Rahmen_Kern“ und „Rahmen_Schein“ der Mana-Glow-Assets): dünne,
/// fast weiße Kernlinie genau an der Kartenkante (Blender: Bevel 0,035 bei Karte 2,5 breit), darum
/// ein breiter Schein (Bevel 0,14) und ein weicher Halo als Ersatz für Blenders Bloom. Additiv
/// (vormultipliziert, Deckkraft 0), unbeleuchtet. `strength` = Kern, `schein` = Schein; ruhiges Pulsieren wie das Idle der Datei.
const GLOW_SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, blend_premul_alpha, cull_disabled, depth_draw_never, shadows_disabled, fog_disabled;
uniform vec3 glow_color = vec3(0.02, 0.28, 1.0);
uniform float strength = 1.0;
uniform float schein = 0.3;
uniform vec2 quad_size;
uniform vec2 card_half;

float sd_round_rect(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + vec2(r);
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}

void fragment() {
    vec2 p = (UV - 0.5) * quad_size;
    float edge = sd_round_rect(p, card_half, 0.038);
    float core = 1.0 - smoothstep(0.004, 0.012, abs(edge - 0.004));
    float sch = exp(-pow(abs(edge - 0.013) / 0.035, 2.0));
    float halo = exp(-max(edge, 0.0) / 0.06) * 0.45;
    float pulse = 0.85 + 0.15 * sin(TIME * 5.0);
    float margin = quad_size.x * 0.5 - card_half.x;
    float fade = smoothstep(margin, margin * 0.55, max(edge, 0.0));
    vec3 col = mix(glow_color, vec3(1.0), core * 0.35);
    ALBEDO = col * (core * strength * 1.4 + (sch + halo) * schein * strength) * pulse * fade;
    ALPHA = 0.0;
}
"#;

/// Eine Spielkarte auf dem 3D-Tisch: Mesh mit Vorder-/Rückseite und Beschriftung,
/// Hover-Neigung und Hover-Zoom (offene Karten werden groß und lesbar), Raycast-Ziel
/// (Kollisions-Layer 1), Gleit-Animationen per Tween sowie alle Spielwerte
/// (Name, Typzeile, Kosten, Regeltext, Werte aus den Kartendaten der Engine; Tapp-Zustand).
#[derive(GodotClass)]
#[class(base=Area3D)]
pub struct Card3D {
    /// Textur der Vorderseite.
    #[export]
    #[var(set = set_face_texture)]
    face_texture: Option<Gd<Texture2D>>,
    /// Textur der Rückseite.
    #[export]
    #[var(set = set_back_texture)]
    back_texture: Option<Gd<Texture2D>>,
    /// Maximale Neigung bei Mouseover (Grad).
    #[export]
    max_tilt_degrees: f32,
    /// Glättung von Tilt/Zoom (größer = schneller).
    #[export]
    tilt_speed: f32,
    /// Dauer der Gleit-Animation zu einem Slot (Sekunden).
    #[export]
    move_duration: f64,

    #[export_group(name = "Hover-Zoom")]
    /// Offene Karten beim Hovern selbst groß und lesbar vor die Kamera holen (setzt die Zone,
    /// [member Zone.zoom_cards]: an für die eigene Hand; sonst zeigt der [CardInspector] eine Kopie
    /// mit Fähigkeiten und Buffs).
    #[export]
    zoom_enabled: bool,
    /// Anteil der Bildschirmhöhe, den die gezoomte Karte einnimmt.
    #[export(range = (0.1, 0.95))]
    zoom_screen_height: f32,
    /// Abstand der gezoomten Karte zur Kamera (liegt damit vor allem anderen).
    #[export]
    zoom_distance: f32,
    /// Rand zum Bildschirmrand in Pixeln.
    #[export]
    zoom_margin_px: f32,
    /// Anheben (entlang der Kartennormalen) für verdeckte Karten / ohne Zoom.
    #[export]
    hover_lift: f32,
    /// Skalierung für verdeckte Karten / ohne Zoom.
    #[export]
    hover_scale: f32,

    /// Kartenname (wie von der Engine).
    #[export_group(name = "Karte")]
    #[export]
    card_name: GString,
    /// Kartenart: 0 = Kreatur, 1 = Land, 2 = Spontanzauber, 3 = Hexerei, 4 = Planeswalker,
    /// 5 = Ausrüstung, 6 = Artefakt, 7 = Verzauberung, 8 = Aura, 9 = Schlacht.
    #[export]
    card_kind: CardKind,
    /// Kartendaten für das Bild (Typzeile, Regeltext, Werte), siehe [`CardDef`].
    def: CardDef,

    // Anzeige-Zustand
    tapped: bool,
    marker: CardMarker,
    /// Dezentes Leuchten in dieser Farbe: Karte kann gerade handeln (None = aus).
    ready_glow: Option<Color>,
    /// Aufleuchten seit (Sekunden; `None` = keins): Rahmen kurz hell und orange.
    flash_t: Option<f32>,
    /// Angreifer im Kampf: rückt vor und trägt die Angriffsspitze.
    advanced: bool,
    advance_amount: f32,
    attack_banner: Option<Gd<MeshInstance3D>>,
    banner_material: Option<Gd<ShaderMaterial>>,
    /// Blocker im Kampf: Schweif über der Kante zum Angreifer ([method set_block_trail]), gezeigte
    /// Stärke 0 … 1.
    block_trail: bool,
    trail_amount: f32,
    trail_mesh: Option<Gd<MeshInstance3D>>,
    trail_material: Option<Gd<ShaderMaterial>>,
    /// In einem erklärten Block ([method set_block_declared]): Rahmen, Angriffsspitze und Schweif
    /// gelb; gezeigter Anteil Gelb 0 … 1.
    block_declared: bool,
    yellow_amount: f32,
    /// Schlag zum Ziel (Welt), Zeit, Reichweite (Anteil des Wegs), Verzögerung.
    lunge: Option<(Vector3, f32, f32, f32)>,
    lunge_applied: Vector3,
    /// Drehung des Schlags in diesem Frame (wird vor dem nächsten Frame zurückgenommen).
    lunge_turn_applied: f32,
    /// Restzeit (Sekunden), in der die Karte an ihrem Platz bleibt ([method hold]).
    hold_left: f32,
    /// Kartenbild um 180° gedreht (Karten des Gegners lesbar von unserer Seite), siehe
    /// [method set_turned].
    turned: bool,
    /// Im Kampf geblockt bzw. blockend: liegt auf dem Spielfeld in einem eigenen Stapel.
    combat_alone: bool,
    /// Geblockter Angreifer: Welt-x seines Blockers (die Zone legt ihn genau gegenüber).
    combat_x: Option<f32>,
    /// Rahmenfarbe: gezeigt und Ziel (weicher Übergang, [`GLOW_FADE_SPEED`]).
    glow_shown: Color,
    glow_goal: Color,
    /// Kachel-Form gewünscht (Spielfeld) bzw. gerade gezeigt (beim Zoomen/Ziehen ganze Karte).
    tile: bool,
    tile_shown: bool,
    shape_refs: Option<ShapeRefs>,
    /// Immer unbeleuchtet in Originalfarben (Karten auf einer 3D-Bühne vor der Kamera, wo das
    /// Tischlicht nicht hinreicht – sonst wirken sie grau).
    bright: bool,
    /// Hat in diesem Zug angegriffen (ausgegraut, bis sie enttappt wird).
    attacked: bool,
    /// Laufendes Umdrehen: vergangene Zeit und die neue Seite (Kartendaten, Bild), die bei
    /// halber Drehung übernommen wird.
    flip: Option<(f32, Option<(CardDef, Gd<Texture2D>)>)>,
    /// Besitzer (0 = Spieler, 1 = Gegner): auf dessen Friedhof geht die Karte.
    #[var]
    pub(crate) owner_seat: i64,

    labels: Option<CardLabels>,
    /// Text der Plaketten (eine Zeile je Marke bzw. Kennzeichen).
    badges: String,
    marker_mesh: Option<Gd<MeshInstance3D>>,
    marker_material: Option<Gd<ShaderMaterial>>,
    tap_icon: Option<Gd<MeshInstance3D>>,
    visual: Option<Gd<Node3D>>,
    /// Marken als Münzen rechts oben am Rand (`set_counters`): Markenart, Anzahl, Münze,
    /// Zeit seit dem Zuwachs (für Aufspringen und blaues Leuchten; ≥ COIN_POP_TIME = ruhig).
    coins: Vec<(String, u32, Gd<godot::classes::Sprite3D>, f32)>,
    /// Loyalitäts-Schild eines Planeswalkers auf dem Spielfeld (wie in Arena unten mittig an der
    /// Kachel): Loyalität, Schild, Zeit seit der letzten Änderung (Aufblitzen; ≥ SHIELD_FLASH_TIME = ruhig).
    shield: Option<(i32, Gd<godot::classes::Sprite3D>, f32)>,
    /// Kachel-Textur für das Spielfeld (`placeholder::tile_face`), sonst None (dann der obere
    /// Teil der Vorderseite).
    tile_texture: Option<Gd<Texture2D>>,
    /// Regeltext mit eingebetteten Manasymbolen (`set_rich_text`), statt der Beschriftung.
    rich_text: Option<Gd<godot::classes::Sprite3D>>,
    /// Feld über dem gedruckten Textfeld für eigenen Text einer Bühne (`set_text_panel`).
    text_panel: Option<Gd<MeshInstance3D>>,
    /// Kostensymbole über der Oberkante (nur in Zonen mit `show_costs`): Kosten-Text und Reihe.
    cost_row: Option<(String, Gd<Node3D>)>,
    meshes: Vec<Gd<MeshInstance3D>>,
    face_material: Option<Gd<StandardMaterial3D>>,
    back_material: Option<Gd<StandardMaterial3D>>,
    hovered: bool,
    zoomed: bool,
    /// Mausposition relativ zur Karte, normalisiert auf -1..1 (x = links/rechts, y = oben/unten).
    hover_local: Vector2,
    dragging: bool,
    /// Neigung während des Ziehens (vom Controller aus der Bewegung berechnet), Radiant.
    drag_sway: Vector2,
    tween: Option<Gd<Tween>>,
    zone: Option<Gd<Zone>>,

    base: Base<Area3D>,
}

#[godot_api]
impl IArea3D for Card3D {
    fn init(base: Base<Area3D>) -> Self {
        Self {
            face_texture: None,
            back_texture: None,
            max_tilt_degrees: 10.0,
            tilt_speed: 16.0,
            move_duration: 0.35,
            zoom_enabled: false,
            zoom_screen_height: 0.55,
            zoom_distance: 2.0,
            zoom_margin_px: 12.0,
            hover_lift: 0.06,
            hover_scale: 1.08,
            card_name: "Karte".into(),
            card_kind: CardKind::Creature,
            def: CardDef::named("Karte"),
            tapped: false,
            marker: CardMarker::None,
            ready_glow: None,
            flash_t: None,
            advanced: false,
            advance_amount: 0.0,
            attack_banner: None,
            banner_material: None,
            block_trail: false,
            trail_amount: 0.0,
            trail_mesh: None,
            trail_material: None,
            block_declared: false,
            yellow_amount: 0.0,
            lunge: None,
            lunge_applied: Vector3::ZERO,
            lunge_turn_applied: 0.0,
            hold_left: 0.0,
            turned: false,
            combat_alone: false,
            combat_x: None,
            glow_shown: PLAYABLE_GLOW,
            glow_goal: PLAYABLE_GLOW,
            tile: false,
            tile_shown: false,
            shape_refs: None,
            bright: false,
            attacked: false,
            flip: None,
            owner_seat: 0,
            labels: None,
            badges: String::new(),
            marker_mesh: None,
            marker_material: None,
            tap_icon: None,
            visual: None,
            meshes: Vec::new(),
            face_material: None,
            back_material: None,
            hovered: false,
            zoomed: false,
            hover_local: Vector2::ZERO,
            dragging: false,
            drag_sway: Vector2::ZERO,
            tween: None,
            zone: None,
            cost_row: None,
            text_panel: None,
            rich_text: None,
            tile_texture: None,
            coins: Vec::new(),
            shield: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.build_nodes();
    }

    fn process(&mut self, delta: f64) {
        if self.visual.is_none() {
            return;
        }

        let max_tilt = self.max_tilt_degrees.to_radians();
        // Die Kante unter dem Mauszeiger wird "hineingedrückt":
        // +X-Rotation senkt die +Z-Kante, +Z-Rotation hebt die +X-Kante.
        let tilt = Basis::from_euler(
            EulerOrder::XYZ,
            Vector3::new(self.hover_local.y * max_tilt, 0.0, -self.hover_local.x * max_tilt),
        );

        let mut zoomed = false;
        let target = if self.dragging {
            let sway = Vector3::new(self.drag_sway.y, 0.0, self.drag_sway.x);
            Transform3D::new(Basis::from_euler(EulerOrder::XYZ, sway), Vector3::ZERO)
        } else if self.hovered {
            let card_xf = self.base().get_global_transform();
            match self.zoom_transform(&card_xf) {
                Some(mut global) => {
                    zoomed = true;
                    global.basis = global.basis * tilt;
                    card_xf.affine_inverse() * global
                }
                None => Transform3D::new(
                    tilt.scaled(Vector3::splat(self.hover_scale)) * self.turn_basis(),
                    Vector3::new(0.0, self.hover_lift, 0.0),
                ),
            }
        } else {
            Transform3D::new(self.turn_basis(), Vector3::ZERO)
        };

        if zoomed != self.zoomed {
            self.zoomed = zoomed;
            self.apply_brightness();
            // Die schwebende Karte soll keinen riesigen Schatten auf den Tisch werfen.
            let setting = if zoomed { ShadowCastingSetting::OFF } else { ShadowCastingSetting::ON };
            for mesh in self.meshes.iter_mut() {
                mesh.set_cast_shadows_setting(setting);
            }
        }

        self.update_cost_row();
        let dt = delta as f32;
        // Münzen und Schild nur, solange sie aufspringen bzw. aufblitzen (sonst stehen sie still)
        if self.coins.iter().any(|c| c.3 < COIN_POP_TIME) {
            self.update_coins(dt);
        }
        if self.shield.as_ref().is_some_and(|s| s.2 < SHIELD_FLASH_TIME) {
            self.update_shield(dt);
        }
        // Kachel auf dem Spielfeld, beim Zoomen und Ziehen die ganze Karte (lesbar)
        let want_tile = if self.hold_left > 0.0 { self.tile_shown } else { self.tile && !zoomed && !self.dragging };
        if want_tile != self.tile_shown {
            self.tile_shown = want_tile;
            self.apply_shape();
        }
        self.hold_left = (self.hold_left - dt).max(0.0);
        if self.glow_shown != self.glow_goal {
            let w = 1.0 - (-GLOW_FADE_SPEED * dt).exp();
            let c = self.glow_shown.lerp(self.glow_goal, w as f64);
            let close = (c.r - self.glow_goal.r).abs() + (c.g - self.glow_goal.g).abs() + (c.b - self.glow_goal.b).abs() < 0.01;
            self.glow_shown = if close { self.glow_goal } else { c };
            let shown = self.glow_shown;
            if let Some(mat) = self.marker_material.as_mut() {
                mat.set_shader_parameter("glow_color", &Vector3::new(shown.r, shown.g, shown.b).to_variant());
            }
        }
        self.update_block_look(dt);
        // Aufleuchten abklingen lassen
        if let Some(t) = self.flash_t.as_mut() {
            *t += dt;
            if *t > FLASH_KEYS[FLASH_KEYS.len() - 1].0 {
                self.flash_t = None;
            }
            self.apply_glow();
        }
        // Angreifer rückt vor (nicht beim Ziehen/Zoomen)
        // einem Blocker gegenüber ([method set_combat_x]) bleibt der Angreifer in seiner Reihe: der
        // Abstand zum Blocker bleibt wie zwischen den Reihen, die Karten überlappen nicht
        let adv_goal = if self.advanced && self.combat_x.is_none() && !self.dragging && !zoomed { 1.0 } else { 0.0 };
        self.advance_amount += (adv_goal - self.advance_amount) * (1.0 - (-10.0 * dt).exp());
        if (adv_goal - self.advance_amount).abs() < 1e-4 {
            self.advance_amount = adv_goal;
        }
        let mut target = target;
        let scale = self.base().get_global_transform().basis.col_a().length().max(0.01);
        target.origin.z -= ADVANCE.min(ADVANCE_MAX_WORLD / scale) * self.advance_amount;
        let (lunge, turn) = self.lunge_offset(dt);

        let w = 1.0 - (-self.tilt_speed * dt).exp();
        let flip = self.advance_flip(dt);
        let applied = self.lunge_applied;
        let turn_applied = self.lunge_turn_applied;
        let visual = self.visual.as_mut().unwrap();
        let shown = visual.get_transform();
        let mut current = shown;
        current.origin -= applied;
        current.basis = Basis::from_axis_angle(Vector3::UP, -turn_applied) * current.basis;
        let mut next = current.interpolate_with(&target, w);
        // Am Ziel einrasten: danach bleibt das Kartenbild unverändert und wird nicht mehr gesetzt
        // (jedes Setzen erneuert alle Teile der Karte im RenderingServer, auch ihre Schatten)
        if transforms_close(&next, &target) {
            next = target;
        }
        next.origin += lunge;
        next.basis = Basis::from_axis_angle(Vector3::UP, turn) * next.basis;
        match flip {
            // Umdrehen: Drehung um die Längsachse (lokal z) und Anheben direkt setzen
            Some(xf) => visual.set_transform(target * xf),
            None if next != shown => visual.set_transform(next),
            None => {}
        }
        self.lunge_applied = lunge;
        self.lunge_turn_applied = turn;
    }
}

#[godot_api]
impl Card3D {
    /// Setzt die Textur der Vorderseite.
    #[func]
    pub fn set_face_texture(&mut self, tex: Option<Gd<Texture2D>>) {
        self.face_texture = tex;
        self.apply_face_material();
        self.apply_tapped_look();
    }

    /// Setzt die Textur der Rückseite.
    #[func]
    pub fn set_back_texture(&mut self, tex: Option<Gd<Texture2D>>) {
        self.back_texture = tex;
        if let Some(mat) = self.back_material.as_mut() {
            apply_texture(mat, self.back_texture.as_ref(), Color::from_rgb(0.15, 0.2, 0.45));
        }
    }

    /// Aktiviert/deaktiviert den Hover-Zustand.
    #[func]
    pub fn set_hovered(&mut self, hovered: bool) {
        self.hovered = hovered;
        if !hovered {
            self.hover_local = Vector2::ZERO;
        }
    }

    /// Liegt der Mauszeiger gerade auf der Karte?
    #[func]
    pub fn is_hovered(&self) -> bool {
        self.hovered
    }

    /// Wird die Karte gerade vergrößert angezeigt?
    #[func]
    pub fn is_zoomed(&self) -> bool {
        self.zoomed
    }

    /// Die vier Ecken der Karte, wie sie gerade auf dem Bildschirm erscheint (inklusive
    /// Hover-Zoom, Neigung und Drag-Schwung), in Pixeln. Leer, wenn die Karte hinter der Kamera liegt.
    #[func]
    pub fn screen_quad(&self) -> PackedVector2Array {
        let (Some(visual), Some(cam)) = (self.visual.as_ref(), self.base().get_viewport().and_then(|v| v.get_camera_3d())) else {
            return PackedVector2Array::new();
        };
        let xf = visual.get_global_transform();
        let (w, h) = (CARD_WIDTH * 0.5, CARD_HEIGHT * 0.5);
        let mut quad = PackedVector2Array::new();
        for (x, z) in [(-w, -h), (w, -h), (w, h), (-w, h)] {
            let world = xf * Vector3::new(x, 0.0, z);
            if cam.is_position_behind(world) {
                return PackedVector2Array::new();
            }
            quad.push(cam.unproject_position(world));
        }
        quad
    }

    /// Übergibt den Raycast-Trefferpunkt (Weltkoordinaten) für den Tilt-Effekt.
    #[func]
    pub fn update_hover_point(&mut self, world_point: Vector3) {
        let local = self.base().get_global_transform().affine_inverse() * world_point;
        self.hover_local = Vector2::new(
            (local.x / (CARD_WIDTH * 0.5)).clamp(-1.0, 1.0),
            (local.z / (CARD_HEIGHT * 0.5)).clamp(-1.0, 1.0),
        );
    }

    /// Markiert die Karte als gezogen (Hover aus, laufende Animation stoppt). Setzt der [TableController].
    #[func]
    pub fn set_dragging(&mut self, dragging: bool) {
        self.dragging = dragging;
        self.apply_glow();
        if dragging {
            self.hovered = false;
            self.stop_motion();
        } else {
            self.drag_sway = Vector2::ZERO;
        }
    }

    /// Wird die Karte gerade gezogen?
    #[func]
    pub fn is_dragging(&self) -> bool {
        self.dragging
    }

    /// Neigung während des Ziehens (x = Rollen um Z, y = Nicken um X), Radiant.
    #[func]
    pub fn set_drag_sway(&mut self, sway: Vector2) {
        self.drag_sway = sway;
    }

    /// Lässt die Karte per Tween sanft zu `target` (globale Transform) gleiten.
    #[func]
    pub fn move_to(&mut self, target: Transform3D, delay: f64) {
        self.stop_motion();
        // im Kampf getroffen bzw. schlagend: erst nach dem Treffer losziehen
        let delay = delay.max(f64::from(self.hold_left));
        let duration = self.move_duration;
        let this = self.to_gd();
        let mut tween = self.base_mut().create_tween();
        tween.set_trans(TransitionType::CUBIC);
        tween.set_ease(EaseType::OUT);
        if delay > 0.0 {
            tween.tween_interval(delay);
        }
        tween.tween_property(&this, "global_transform", &target.to_variant(), duration);
        self.tween = Some(tween);
    }

    /// Setzt die Karte sofort ohne Animation.
    #[func]
    pub fn snap_to(&mut self, target: Transform3D) {
        self.stop_motion();
        self.base_mut().set_global_transform(target);
    }

    /// Bricht eine laufende Gleit-Animation ab.
    #[func]
    pub fn stop_motion(&mut self) {
        if let Some(mut tween) = self.tween.take() {
            if tween.is_valid() {
                tween.kill();
            }
        }
    }

    /// Dezenter goldener Rand „diese Karte kann eine Fähigkeit aktivieren“ an/aus (für Skripte;
    /// Rot/Blau setzt das Spiel selbst). Eine Markierung hat Vorrang.
    #[func]
    pub fn set_ready_glow(&mut self, on: bool) {
        self.set_ready_color(on.then_some(PLAYABLE_GLOW));
    }

    /// Rahmen leuchtet kurz hell und orange auf (etwas wurde aktiviert: gespielt, Fähigkeit,
    /// Mana-Einschlag, Angriff …), danach zurück zum normalen Leuchten.
    #[func]
    pub fn flash(&mut self) {
        self.flash_t = Some(0.0);
        self.apply_glow();
    }

    /// Angreifer im Kampf: Karte rückt etwas zum Gegner vor und trägt die rote Angriffsspitze
    /// an der Oberkante (`false` = zurück an ihren Platz).
    #[func]
    pub fn set_attacking(&mut self, attacking: bool) {
        self.advanced = attacking;
        if let Some(b) = self.attack_banner.as_mut() {
            b.set_visible(attacking);
        }
    }

    /// Blocker im Kampf (Video „Blocken“): über der Kante zum Angreifer steigt ein flackernder
    /// Schweif als Bogen auf – orange, solange der Block geplant ist, gelb, sobald er erklärt ist
    /// ([method set_block_declared]). Blendet in ≈ 0,25 s ein und aus.
    #[func]
    pub fn set_block_trail(&mut self, on: bool) {
        self.block_trail = on;
    }

    /// In einem erklärten Block (Angreifer und Blocker, CR 509.1): Rahmen, Angriffsspitze und
    /// Blockschweif werden gelb ([`BLOCKED_GLOW`]), bis der Kampf endet (`false`).
    #[func]
    pub fn set_block_declared(&mut self, on: bool) {
        if self.block_declared != on {
            self.block_declared = on;
            self.apply_glow();
        }
    }

    /// Kartenbild um 180° gedreht zeigen (setzt die Zone, [member Zone.turn_cards]): Karten des
    /// Gegners liegen in seiner um 180° gedrehten Zone und wären sonst von unserer Seite aus
    /// kopfüber. Nur das Bild dreht sich; Vorrücken und Schlag im Kampf bleiben zum Gegner gerichtet.
    #[func]
    pub fn set_turned(&mut self, turned: bool) {
        if self.turned != turned {
            self.turned = turned;
            self.place_banner();
        }
    }

    /// Im Kampf geblockt bzw. blockend (`true`): die Karte bildet auf dem Spielfeld einen eigenen
    /// Stapel, statt mit gleichen Karten zusammenzuliegen. Gibt zurück, ob sich etwas geändert hat.
    #[func]
    pub fn set_combat_alone(&mut self, alone: bool) -> bool {
        let changed = self.combat_alone != alone;
        self.combat_alone = alone;
        changed
    }

    /// Geblockter Angreifer: liegt seinem Blocker auf dem Spielfeld genau gegenüber (`x` = Welt-x
    /// des Blockers bzw. die Mitte mehrerer Blocker; `on = false` = wieder normal einreihen).
    /// Gibt zurück, ob sich die Lage spürbar geändert hat (dann muss die Zone neu anordnen).
    #[func]
    pub fn set_combat_x(&mut self, x: f32, on: bool) -> bool {
        let new = on.then_some(x);
        let changed = match (self.combat_x, new) {
            (Some(a), Some(b)) => (a - b).abs() > 0.03,
            (a, b) => a.is_some() != b.is_some(),
        };
        if changed {
            self.combat_x = new;
        }
        changed
    }

    /// Welt-x des Blockers, dem diese Karte gegenüberliegt ([method set_combat_x]).
    pub fn combat_x(&self) -> Option<f32> {
        self.combat_x
    }

    /// Kachel-Form (Spielfeld): nur Name und Bild, Stärke/Widerstand unten rechts; beim Zoomen
    /// und Ziehen wird die ganze Karte gezeigt. Setzt die Zone beim Anordnen.
    #[func]
    pub fn set_tile(&mut self, tile: bool) {
        if self.tile != tile {
            self.tile = tile;
            // während [method hold] behält die Karte ihre Form (process wechselt danach)
            if self.hold_left <= 0.0 {
                self.tile_shown = tile && !self.zoomed && !self.dragging;
                self.apply_shape();
            }
        }
    }

    /// Immer hell in Originalfarben zeigen (Karten auf einer 3D-Bühne vor der Kamera).
    #[func]
    pub fn set_bright(&mut self, bright: bool) {
        self.bright = bright;
        self.apply_brightness();
    }

    /// Beim Hovern selbst groß vor die Kamera holen (Hand) – setzt die Zone beim Anordnen.
    #[func]
    pub fn set_zoom(&mut self, zoom: bool) {
        self.zoom_enabled = zoom;
    }

    /// Wird die Karte als Kachel gezeigt?
    #[func]
    pub fn is_tile(&self) -> bool {
        self.tile
    }

    /// Ist die Karte als Angreifer markiert (vorgerückt, mit Angriffsspitze)?
    #[func]
    pub fn is_attacking(&self) -> bool {
        self.advanced
    }

    /// Schlag im Kampf: die Karte holt zum Weltpunkt `target` aus (Anteil `reach` des Wegs,
    /// angehoben), trifft nach [`LUNGE_OUT`] Sekunden und kehrt zurück; Start nach `delay`.
    #[func]
    pub fn lunge(&mut self, target: Vector3, reach: f32, delay: f32) {
        self.lunge = Some((target, 0.0, reach, delay));
        self.hold(delay + LUNGE_OUT + LUNGE_HOLD);
    }

    /// Die Karte bleibt `seconds` lang an ihrem Platz: Bewegungen ([method move_to]) beginnen
    /// erst danach (getroffene Kreatur im Kampf, damit der Schlag sichtbar bleibt).
    #[func]
    pub fn hold(&mut self, seconds: f32) {
        self.hold_left = self.hold_left.max(seconds);
    }

    /// Aura um die Karte setzen: 0 = keine, 1 = Angreifer (rot), 2 = Blocker (blau),
    /// 3 = ausgewählt/wählt ein Ziel (gold).
    #[func]
    pub fn set_marker_kind(&mut self, kind: i64) {
        let marker = match kind {
            1 => CardMarker::Attacking,
            2 => CardMarker::Blocking,
            3 => CardMarker::Selected,
            _ => CardMarker::None,
        };
        self.set_marker(marker);
    }

    /// Setzt nur den Tapp-Zustand der Karte (schräg, ausgegraut, Tapp-Symbol), ohne Spielregeln
    /// (kein Mana, kein Neu-Anordnen) – für Skripte und Tests.
    #[func]
    pub fn set_tapped_state(&mut self, tapped: bool) {
        self.set_tapped(tapped);
    }

    /// Ist die Karte getappt (leicht schräg, ausgegraut, Tapp-Symbol)?
    #[func]
    pub fn is_tapped(&self) -> bool {
        self.tapped
    }

    /// Die Zone, in der die Karte aktuell liegt (None während des Ziehens).
    #[func]
    pub fn get_zone(&self) -> Option<Gd<Zone>> {
        self.zone.clone()
    }
}

impl Card3D {
    // ------------------------------------------------------------ Spielwerte

    /// Übernimmt die Kartendaten (Name, Typ, Text, Werte) und beschriftet die Vorderseite neu.
    pub fn apply_def(&mut self, def: &CardDef) {
        self.card_name = def.name.as_str().into();
        self.card_kind = def.kind;
        self.def = def.clone();
        self.tile_texture = placeholder::tile_face(def);
        if self.tile_shown {
            self.apply_face_material();
        }
        self.refresh_labels();
        self.sync_shield();
    }

    /// Name der Karte (wie auf dem Kartenbild, ohne Übersetzung).
    pub fn card_name(&self) -> GString {
        self.card_name.clone()
    }

    pub fn kind(&self) -> CardKind {
        self.card_kind
    }

    /// Dreht die Karte animiert auf eine andere Seite um (doppelseitige Karten, CR 711): bei
    /// halber Drehung werden `def` und `face` übernommen. Ohne Szene (noch nicht bereit) sofort.
    pub fn flip_to(&mut self, def: &CardDef, face: Gd<Texture2D>) {
        if self.visual.is_none() {
            self.set_face_texture(Some(face));
            self.apply_def(def);
            return;
        }
        // Läuft schon ein Umdrehen, wird nur die Zielseite ersetzt
        let elapsed = self.flip.as_ref().map_or(0.0, |f| f.0);
        self.flip = Some((elapsed, Some((def.clone(), face))));
    }

    /// Schreitet das Umdrehen fort; liefert die Zusatz-Transformation des Kartenbilds (None =
    /// kein Umdrehen). Erste Hälfte: 0° → 90°; dann Seitenwechsel; zweite Hälfte −90° → 0°.
    fn advance_flip(&mut self, delta: f32) -> Option<Transform3D> {
        let (t, _) = self.flip.as_mut()?;
        *t += delta;
        let p = (*t / FLIP_DURATION).min(1.0);
        // Cubic-In-Out
        let e = if p < 0.5 { 4.0 * p * p * p } else { 1.0 - (-2.0 * p + 2.0).powi(3) * 0.5 };
        if e >= 0.5 {
            if let Some((def, face)) = self.flip.as_mut().and_then(|f| f.1.take()) {
                self.set_face_texture(Some(face));
                self.apply_def(&def);
            }
        }
        if p >= 1.0 {
            self.flip = None;
            return Some(Transform3D::IDENTITY);
        }
        let angle = if e < 0.5 { std::f32::consts::PI * e } else { std::f32::consts::PI * (e - 1.0) };
        let lift = (std::f32::consts::PI * p).sin() * FLIP_LIFT;
        Some(Transform3D::new(Basis::from_axis_angle(Vector3::BACK, angle), Vector3::new(0.0, lift, 0.0)))
    }

    /// Aktuelle Kartendaten des Bildes.
    pub fn def(&self) -> &CardDef {
        &self.def
    }

    /// Schlüssel zum Stapeln gleicher Karten auf dem Spielfeld (wie gleichnamige Länder): nur
    /// Karten mit gleichem Namen, Tapp-Zustand, Angriff, Einsatzbereitschaft, Stärke/Widerstand,
    /// Schaden, Marken und Seite liegen auf einem Stapel; getappte, angreifende und noch nicht
    /// einsatzbereite (summoning sick) Kreaturen bilden je einen eigenen; geblockte Angreifer und
    /// Blocker ([method set_combat_alone]) liegen allein.
    pub fn stack_key(&self) -> String {
        let counters: Vec<String> = self.coins.iter().map(|(k, n, _, _)| format!("{k}{n}")).collect();
        // geblockt bzw. blockend: eigener Stapel (eindeutiger Name)
        let name = if self.combat_alone { format!("{}#{}", self.card_name, self.base().instance_id()) } else { self.card_name.to_string() };
        format!(
            "{}|{}|{}|{}|{:?}|{}|{}|{}|{}",
            name,
            self.tapped,
            // angreifend: erklärt (Forge) oder in der Angriffsauswahl vorgemerkt
            self.advanced || self.def.attacking,
            self.def.sick,
            self.def.stats,
            self.def.damage,
            self.def.face,
            self.def.face_down,
            counters.join(",")
        )
    }

    /// Nur Länder: Manafarbe (Spalte in der Länder-Reihe).
    pub fn produced_mana(&self) -> ManaType {
        self.def.produces
    }

    /// Tapp-Zustand setzen; die Drehung übernimmt die Zone beim nächsten `layout()`.
    /// Tappt bzw. enttappt die Karte: getappt = leicht schräg (durch die Zone), ausgegraut und mit
    /// Tapp-Symbol. Enttappen beendet auch den Zustand „hat angegriffen“.
    pub fn set_tapped(&mut self, tapped: bool) {
        self.tapped = tapped;
        if !tapped {
            self.attacked = false;
        }
        self.apply_tapped_look();
    }

    /// Dezentes Leuchten „kann handeln“ in `color` (Rot = kann angreifen, Blau = kann blocken,
    /// Gold = kann eine Fähigkeit aktivieren); None = aus. Eine Markierung hat Vorrang.
    pub fn set_ready_color(&mut self, color: Option<Color>) {
        if self.ready_glow != color {
            self.ready_glow = color;
            self.apply_glow();
        }
    }

    /// Ausgrauen (getappt oder hat angegriffen) und Tapp-Symbol (nur getappt ohne Angriff).
    fn apply_tapped_look(&mut self) {
        let tint = if self.tapped || self.attacked { TAPPED_TINT } else { Color::WHITE };
        if let Some(mat) = self.face_material.as_mut() {
            if self.face_texture.is_some() {
                mat.set_albedo(tint);
            }
        }
        if let Some(labels) = self.labels.as_mut() {
            let colors = labels.colors;
            for (l, c) in labels.all().into_iter().zip(colors) {
                l.set_modulate(c * tint);
            }
        }
        if let Some(icon) = self.tap_icon.as_mut() {
            icon.set_visible(self.tapped && !self.attacked);
        }
    }

    pub fn set_marker(&mut self, marker: CardMarker) {
        self.marker = marker;
        self.apply_glow();
    }

    /// Unbeleuchtet in Originalfarben, wenn gezoomt oder [method set_bright]: vor der Kamera liegt
    /// die Karte außerhalb der Tischbeleuchtung und wäre sonst dunkel bzw. grau.
    fn apply_brightness(&mut self) {
        let bright = self.zoomed || self.bright;
        let shading = if bright { ShadingMode::UNSHADED } else { ShadingMode::PER_PIXEL };
        if let Some(mat) = self.face_material.as_mut() {
            mat.set_shading_mode(shading);
        }
        if let Some(labels) = self.labels.as_mut() {
            for l in labels.all() {
                l.set_draw_flag(DrawFlags::SHADED, !bright);
            }
        }
    }

    /// Sichtbare Länge der Karte (ganz bzw. Kachel).
    fn length(&self) -> f32 {
        if self.tile_shown { TILE_HEIGHT } else { CARD_HEIGHT }
    }

    /// Lokale z-Lage zu einer Pixelzeile der Kartentextur (358 hoch), passend zur Form.
    fn px_z(&self, y: f32) -> f32 {
        if self.tile_shown { y / 358.0 * CARD_HEIGHT - TILE_HEIGHT * 0.5 } else { (y / 358.0 - 0.5) * CARD_HEIGHT }
    }

    /// Meshes, Textur-Zuschnitt, Kollision, Rahmen-Glow und Beschriftung an die Form anpassen.
    fn apply_shape(&mut self) {
        let len = self.length();
        let frac = len / CARD_HEIGHT;
        if let Some(r) = self.shape_refs.as_mut() {
            r.body.set_size(Vector3::new(CARD_WIDTH, CARD_THICKNESS, len));
            r.face.set_size(Vector2::new(CARD_WIDTH, len));
            r.back.set_size(Vector2::new(CARD_WIDTH, len));
            r.shape.set_size(Vector3::new(CARD_WIDTH, CARD_THICKNESS * 4.0, len));
            let quad = Vector2::new(CARD_WIDTH, len) + Vector2::splat(2.0 * GLOW_MARGIN);
            r.marker.set_size(quad);
        }
        if let Some(m) = self.back_material.as_mut() {
            m.set_uv1_scale(Vector3::new(1.0, frac, 1.0));
        }
        self.apply_face_material();
        if let Some(m) = self.marker_material.as_mut() {
            let quad = Vector2::new(CARD_WIDTH, len) + Vector2::splat(2.0 * GLOW_MARGIN);
            m.set_shader_parameter("quad_size", &quad.to_variant());
            m.set_shader_parameter("card_half", &(Vector2::new(CARD_WIDTH, len) * 0.5).to_variant());
        }
        self.place_banner();
        let icon_z = if self.tile_shown { 0.0 } else { -CARD_HEIGHT * 0.12 };
        if let Some(i) = self.tap_icon.as_mut() {
            i.set_position(Vector3::new(0.0, CARD_THICKNESS * 0.5 + 0.003, icon_z));
        }
        self.refresh_labels();
        self.sync_shield();
        self.update_coins(0.0);
    }

    /// Vorderseite: als Kachel die eigene Kachel-Textur (ganz), sonst die Kartenvorderseite (als
    /// Kachel nur ihr oberer Teil).
    fn apply_face_material(&mut self) {
        let tile_tex = self.tile_shown.then(|| self.tile_texture.clone()).flatten();
        let frac = self.length() / CARD_HEIGHT;
        let tex = tile_tex.clone().or_else(|| self.face_texture.clone());
        if let Some(m) = self.face_material.as_mut() {
            apply_texture(m, tex.as_ref(), Color::from_rgb(0.9, 0.9, 0.9));
            m.set_uv1_scale(Vector3::new(1.0, if tile_tex.is_some() { 1.0 } else { frac }, 1.0));
        }
    }

    /// Zeigt die Karte gerade ihre Kachel-Textur?
    fn tile_textured(&self) -> bool {
        self.tile_shown && self.tile_texture.is_some()
    }

    /// Versatz (lokal) und Drehung in der Tischebene (Bogenmaß) des Schlags im Kampf für dieses
    /// Frame: die Karte hebt ab, kippt leicht zur Seite und landet beim Treffer über dem Ziel.
    fn lunge_offset(&mut self, dt: f32) -> (Vector3, f32) {
        let Some((target, t, reach, delay)) = self.lunge.as_mut() else { return (Vector3::ZERO, 0.0) };
        if *delay > 0.0 {
            *delay -= dt;
            return (Vector3::ZERO, 0.0);
        }
        *t += dt;
        let k = if *t < LUNGE_OUT {
            let u = *t / LUNGE_OUT;
            u * u // Ausholen beschleunigt
        } else if *t < LUNGE_OUT + LUNGE_BACK {
            let u = (*t - LUNGE_OUT) / LUNGE_BACK;
            1.0 - (1.0 - (1.0 - u).powi(3)) // zurück, weich auslaufend
        } else {
            self.lunge = None;
            return (Vector3::ZERO, 0.0);
        };
        let (target, reach) = (*target, *reach);
        let xf = self.base().get_global_transform();
        let local = xf.affine_inverse() * target;
        let mut off = local * (reach * k);
        // angehoben bis zum Treffer (liegt dann über dem Ziel), beim Rückweg wieder herab
        off.y += LUNGE_LIFT * (std::f32::consts::FRAC_PI_2 * k).sin() / xf.basis.col_a().length().max(0.01);
        // seitlich gekippt, am stärksten in der Mitte des Flugs
        let turn = LUNGE_TURN_DEGREES.to_radians() * (std::f32::consts::PI * k).sin() * reach.min(1.0);
        (off, turn)
    }

    /// Drehung des Kartenbilds ([member turned]).
    fn turn_basis(&self) -> Basis {
        if self.turned { Basis::from_axis_angle(Vector3::UP, std::f32::consts::PI) } else { Basis::IDENTITY }
    }

    /// Angriffsspitze an die Kante zum Gegner legen (bei gedrehtem Kartenbild an die andere Kante,
    /// sonst zeigte sie beim Gegner von uns weg).
    fn place_banner(&mut self) {
        let len = if self.tile_shown { TILE_HEIGHT } else { CARD_HEIGHT };
        let turned = self.turned;
        let basis = if turned { Basis::from_axis_angle(Vector3::UP, std::f32::consts::PI) } else { Basis::IDENTITY };
        let side = if turned { -1.0 } else { 1.0 };
        if let Some(b) = self.attack_banner.as_mut() {
            let z = -len * 0.5 - BANNER_SIZE.y * 0.5 + 0.02;
            b.set_transform(Transform3D::new(basis, Vector3::new(0.0, CARD_THICKNESS, z * side)));
        }
        if let Some(t) = self.trail_mesh.as_mut() {
            let z = -len * 0.5 - TRAIL_HEIGHT * 0.5 + 0.015;
            t.set_transform(Transform3D::new(basis, Vector3::new(0.0, CARD_THICKNESS * 1.5, z * side)));
        }
    }

    /// Blockschweif ein-/ausblenden und Orange → Gelb überblenden (Angriffsspitze und Schweif);
    /// der Rahmen blendet über [`GLOW_FADE_SPEED`] selbst über.
    fn update_block_look(&mut self, dt: f32) {
        let w = 1.0 - (-TRAIL_FADE_SPEED * dt).exp();
        let trail_goal = if self.block_trail && !self.dragging { 1.0 } else { 0.0 };
        let yellow_goal = if self.block_declared { 1.0 } else { 0.0 };
        let step = |v: f32, goal: f32| if (goal - v).abs() < 0.005 { goal } else { v + (goal - v) * w };
        let (trail, yellow) = (step(self.trail_amount, trail_goal), step(self.yellow_amount, yellow_goal));
        if trail != self.trail_amount || yellow != self.yellow_amount {
            self.trail_amount = trail;
            self.yellow_amount = yellow;
            if let (Some(mesh), Some(mat)) = (self.trail_mesh.as_mut(), self.trail_material.as_mut()) {
                mesh.set_visible(trail > 0.0);
                mat.set_shader_parameter("amount", &trail.to_variant());
                mat.set_shader_parameter("yellow", &yellow.to_variant());
            }
            if let Some(mat) = self.banner_material.as_mut() {
                mat.set_shader_parameter("yellow", &yellow.to_variant());
            }
        }
    }

    fn apply_glow(&mut self) {
        let flash = self.flash_t.map(|t| flash_gain(t));
        let active = self.marker != CardMarker::None || self.dragging || flash.is_some();
        let glow = if active {
            Some((ACTIVE_GLOW, MARK_STRENGTH * flash.unwrap_or(1.0)))
        } else if self.block_declared {
            Some((BLOCKED_GLOW, MARK_STRENGTH))
        } else {
            self.ready_glow.map(|c| (c, READY_STRENGTH))
        };
        let active = active || self.block_declared;
        if let (Some(mesh), Some(mat)) = (self.marker_mesh.as_mut(), self.marker_material.as_mut()) {
            let was_visible = mesh.is_visible();
            mesh.set_visible(glow.is_some());
            if let Some((c, strength)) = glow {
                // eben erst sichtbar: gleich in der Zielfarbe, sonst weich überblenden (process)
                self.glow_goal = c;
                if !was_visible {
                    self.glow_shown = c;
                }
                let shown = self.glow_shown;
                mat.set_shader_parameter("glow_color", &Vector3::new(shown.r, shown.g, shown.b).to_variant());
                mat.set_shader_parameter("strength", &strength.to_variant());
                mat.set_shader_parameter("schein", &(if active { 0.45f32 } else { 0.32 }).to_variant());
            }
        }
    }

    /// Alle Texte der Kartenvorderseite in der aktuellen Sprache, mit Positionen in
    /// Pixeln der Kartentextur (256 × 358) – Quelle der 3D-Beschriftung.
    pub fn face_texts(&self) -> FaceTexts {
        let (stats, stats_color) = self.def.stats_text();
        let printed = placeholder::has_printed_face(&self.def);
        let name = self.card_name.to_string();
        let (cost, cost_x, name_x, name_width) = if self.card_kind == CardKind::Land {
            (String::new(), 0.0, 128.0, 216.0)
        } else {
            // Generische Kosten als Zahl im grauen Kostensymbol (ganz links),
            // Name mittig im freien Bereich links der Kostensymbole
            let c = self.def.cost;
            let pips = c.specific.total() as usize + usize::from(c.generic > 0);
            let centers = placeholder::pip_centers(pips);
            let left_pip = centers.first().copied().unwrap_or(256.0 - 20.0);
            // die Zahl steht schon als Symbol im Kartenbild (Symbolbogen) – dann keine Beschriftung
            let cost = if c.generic > 0 && !placeholder::draws_generic(&self.def) { c.generic.to_string() } else { String::new() };
            (cost, centers.first().copied().unwrap_or(0.0), (26.0 + left_pip - 16.0) * 0.5, left_pip - 16.0 - 26.0)
        };
        FaceTexts {
            printed,
            custom_text: self.def.custom_text,
            tile_textured: self.tile_textured(),
            name,
            name_x,
            name_width,
            cost,
            cost_x,
            kind: self.def.type_line.clone(),
            body: self.def.display_text(),
            stats,
            stats_color,
        }
    }

    /// Plaketten über dem Bildfeld der Vorderseite (eine Zeile je Marke bzw. Kennzeichen,
    /// z. B. „+1/+1 ×2“, „Spielstein“); leer = keine.
    /// Marken der Karte (CR 122) als Münzen wie im Video „Choose One + Marke“: je Markenart eine
    /// silberne Münze mit der Anzahl, rechts oben zur Hälfte über den Rand ragend, mehrere
    /// untereinander. Loyalität und Verteidigung stehen im Wertefeld und bekommen keine Münze.
    /// Kommt eine Marke hinzu, springt ihre Münze kurz auf und leuchtet blau.
    pub fn set_counters(&mut self, counters: &[(String, u32)]) {
        let wanted: Vec<(String, u32)> = counters
            .iter()
            .filter(|(k, n)| (*n > 0 || k == "DOORS") && k != "LOYALTY" && k != "DEFENSE")
            .cloned()
            .collect();
        let same = wanted.len() == self.coins.len() && wanted.iter().zip(&self.coins).all(|((k, n), (ck, cn, _, _))| k == ck && n == cn);
        if same {
            return;
        }
        let Some(mut visual) = self.visual.clone() else { return };
        let old: Vec<(String, u32, f32)> = self.coins.iter().map(|(k, n, _, t)| (k.clone(), *n, *t)).collect();
        for (_, _, mut coin, _) in self.coins.drain(..) {
            coin.queue_free();
        }
        for (k, n) in wanted {
            let before = old.iter().find(|(ok, _, _)| *ok == k);
            // neu oder mehr geworden: Animation von vorn; sonst den Stand übernehmen
            let t = match before {
                Some((_, on, ot)) if *on >= n => *ot,
                _ => 0.0,
            };
            let mut coin = cost_symbols::counter_coin(&k, n, COIN_SIZE);
            coin.set_rotation(Vector3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0));
            visual.add_child(&coin);
            self.coins.push((k, n, coin, t));
        }
        self.update_coins(0.0);
    }

    /// Münzen platzieren (rechts oben am sichtbaren Kartenrand, auch als Kachel) und animieren.
    fn update_coins(&mut self, dt: f32) {
        if self.coins.is_empty() {
            return;
        }
        let top = if self.tile_shown { -TILE_HEIGHT * 0.5 } else { -CARD_HEIGHT * 0.5 };
        for (i, (_, _, coin, t)) in self.coins.iter_mut().enumerate() {
            *t = (*t + dt).min(COIN_POP_TIME);
            let p = *t / COIN_POP_TIME;
            let pop = if p < 1.0 { 1.0 + 0.6 * (p * std::f32::consts::PI).sin() } else { 1.0 };
            coin.set_scale(Vector3::splat(pop));
            let glow = if p < 1.0 { 1.0 - p } else { 0.0 };
            coin.set_modulate(Color::WHITE.lerp(Color::from_rgb(0.55, 0.75, 1.6), f64::from(glow)));
            coin.set_position(Vector3::new(
                CARD_WIDTH * 0.5,
                CARD_THICKNESS * 0.5 + 0.006,
                top + COIN_SIZE * 0.7 + i as f32 * COIN_SIZE * 1.08,
            ));
        }
    }

    /// Zeigt die Karte den Loyalitäts-Schild (Planeswalker als Kachel auf dem Spielfeld)?
    fn shows_shield(&self) -> bool {
        self.tile_shown && self.def.kind == CardKind::Planeswalker && self.def.loyalty.is_some() && !self.def.face_down
    }

    /// Loyalitäts-Schild (CR 306.5) anlegen, ändern oder entfernen; ändert sich die Loyalität,
    /// blitzt der Schild auf (springt kurz auf und leuchtet hell).
    fn sync_shield(&mut self) {
        let want = self.shows_shield().then(|| self.def.loyalty.unwrap_or(0));
        match (want, self.shield.as_mut()) {
            (None, Some(_)) => {
                if let Some((_, mut s, _)) = self.shield.take() {
                    s.queue_free();
                }
            }
            (Some(v), Some((old, sprite, t))) if *old != v => {
                if let Some(tex) = cost_symbols::symbol_texture(&format!("%shield:{v}")) {
                    sprite.set_texture(&tex);
                }
                *old = v;
                *t = 0.0;
            }
            (Some(v), None) => {
                let Some(mut visual) = self.visual.clone() else { return };
                let mut sprite = cost_symbols::loyalty_shield(v, SHIELD_SIZE);
                sprite.set_rotation(Vector3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0));
                visual.add_child(&sprite);
                self.shield = Some((v, sprite, SHIELD_FLASH_TIME));
            }
            _ => {}
        }
        self.update_shield(0.0);
    }

    /// Schild unten mittig an der Kachel platzieren (halb über den Rand) und das Aufblitzen animieren.
    fn update_shield(&mut self, dt: f32) {
        let len = self.length();
        let Some((_, sprite, t)) = self.shield.as_mut() else { return };
        *t = (*t + dt).min(SHIELD_FLASH_TIME);
        let p = *t / SHIELD_FLASH_TIME;
        let pop = if p < 1.0 { 1.0 + 0.45 * (p * std::f32::consts::PI).sin() } else { 1.0 };
        sprite.set_scale(Vector3::splat(pop));
        let glow = if p < 1.0 { 1.0 - p } else { 0.0 };
        sprite.set_modulate(Color::WHITE.lerp(Color::from_rgb(1.6, 1.5, 1.1), f64::from(glow)));
        sprite.set_position(Vector3::new(0.0, CARD_THICKNESS * 0.5 + 0.008, len * 0.5 - SHIELD_SIZE * 0.15));
    }

    pub fn set_badges(&mut self, text: &str) {
        if self.badges == text {
            return;
        }
        self.badges = text.to_string();
        if let Some(l) = self.labels.as_mut() {
            style_badge(&mut l.badges, text);
        }
    }

    pub(crate) fn refresh_labels(&mut self) {
        let t = self.face_texts();
        let tile = self.tile_shown;
        let shield = self.shows_shield();
        let zs = [219.0f32, TEXT_BOX_CENTER, 130.0, placeholder::title_center_y()].map(|y| self.px_z(y));
        let len = self.length();
        let Some(l) = self.labels.as_mut() else {
            return;
        };
        let px = |x: f32, z: f32| Vector3::new((x / 256.0 - 0.5) * CARD_WIDTH, CARD_THICKNESS * 0.5 + 0.0015, z);
        // Kachel: nur Name, Plaketten und Stärke/Widerstand (unten rechts); keine Kosten, Typzeile, Text.
        // Echtes Kartenbild: Name, Kosten, Typzeile und Text stehen im Bild; Stärke/Widerstand nur,
        // wenn sie vom Aufdruck abweichen (grün/rot) – sonst verdeckten sie die gedruckten Werte.
        let printed = t.printed;
        // eigener Text auf echtem Bild: nur der Text, auf einem Feld über dem gedruckten Textfeld
        let custom = printed && t.custom_text && !tile;
        let tiled = t.tile_textured;
        l.name.set_visible(!printed || tiled);
        l.kind.set_visible(!tile && !printed);
        l.text.set_visible(!tile && (!printed || custom));
        l.cost.set_visible(!tile && !printed && !t.cost.is_empty());
        // Planeswalker-Kachel: Loyalität steht im Schild, nicht im Wertefeld
        l.stats.set_visible((!printed || tiled || t.stats_color != Color::WHITE) && !shield);
        // echtes Kartenbild mit veränderten Werten: eigenes Wertefeld über dem gedruckten
        let plate = printed && !tile && !tiled && t.stats_color != Color::WHITE && !shield;
        l.stats_plate.set_visible(plate);
        l.kind.set_position(px(128.0, zs[0]));
        l.text.set_position(px(128.0, zs[1]));
        l.badges.set_position(px(128.0, zs[2]));
        l.stats.set_position(if tile {
            px(212.0, len * 0.5 - 0.045)
        } else if plate {
            px(STATS_PLATE_CENTER.x, (STATS_PLATE_CENTER.y / 358.0 - 0.5) * CARD_HEIGHT)
        } else {
            px(206.0, (333.0 / 358.0 - 0.5) * CARD_HEIGHT)
        });
        let ty = zs[3];
        // Schriftgrößen an die Textlänge anpassen. Umrechnung: 1 Schriftpunkt = PIXEL m,
        // 1 Texturpixel = CARD_WIDTH / 256 m.
        let per_px = (CARD_WIDTH / 256.0) / LABEL_PIXEL;
        l.name.set_text(&t.name);
        l.name.set_font_size(card_text::fit_line(&t.name, t.name_width * per_px, 16, 36));
        l.name.set_position(px(t.name_x, ty));
        if tiled {
            // Kachel: Name in der Namensleiste (dunkel, links), Werte im Kästchen unten mittig
            let tz = |y: i32| (y as f32 / placeholder::TILE_H as f32 - 0.5) * len;
            l.name.set_font_size(card_text::fit_line(&t.name, 230.0 * per_px, 20, 54));
            l.name.set_position(px(128.0, tz(placeholder::TILE_NAME_Y)));
            l.stats.set_position(px(128.0, tz(placeholder::TILE_STATS_Y)));
        }
        l.kind.set_font_size(card_text::fit_line(&t.kind, 216.0 * per_px, 14, 34));
        // Textfeld: 0,52 m breit (Umbruch), ca. 90 Texturpixel hoch
        let area = (0.52 / LABEL_PIXEL) * (TEXT_BOX_HEIGHT * per_px);
        if custom {
            // eigener Text auf dem größeren Feld über dem gedruckten Textfeld: größere Schrift
            let panel = (0.52 / LABEL_PIXEL) * (CARD_HEIGHT * TEXT_PANEL_HEIGHT * 0.8 / LABEL_PIXEL);
            l.text.set_font_size(card_text::fit_font(&t.body, panel, 14, 56));
            l.text.set_position(px(128.0, (TEXT_PANEL_CENTER - 0.5) * CARD_HEIGHT));
        } else if t.custom_text {
            // eigener Text einer Bühne im gezeichneten Rahmen: größer, das Textfeld gehört ihm allein
            l.text.set_font_size(card_text::fit_font(&t.body, area, 12, 44));
        } else {
            l.text.set_font_size(card_text::fit_font(&t.body, area, 10, 30));
        }
        l.cost.set_text(&t.cost);
        if !t.cost.is_empty() {
            l.cost.set_position(px(t.cost_x, ty));
        }
        l.kind.set_text(&t.kind);
        l.text.set_text(&t.body);
        l.stats.set_text(&t.stats);
        l.colors[4] = t.stats_color;
        let tint = if self.tapped || self.attacked { TAPPED_TINT } else { Color::WHITE };
        if tiled {
            // dunkle Schrift im hellen Kästchen; verändert: blau (höher) bzw. rot (niedriger)
            let c = if t.stats_color == Color::WHITE {
                Color::from_rgb(0.1, 0.1, 0.12)
            } else if t.stats_color.g > t.stats_color.r {
                Color::from_rgb(0.15, 0.3, 0.95)
            } else {
                Color::from_rgb(0.85, 0.15, 0.12)
            };
            l.colors[4] = c;
            l.stats.set_font_size(card_text::fit_line(&t.stats, 76.0 * per_px, 30, 70));
            l.colors[0] = Color::from_rgb(0.08, 0.08, 0.09);
            l.stats.set_modulate(c * tint);
            l.stats.set_outline_size(0);
            l.name.set_modulate(l.colors[0] * tint);
        } else if plate {
            // auf dem eigenen Wertefeld: dunkle Schrift wie gedruckt, verändert blau bzw. rot
            let c = if t.stats_color.g > t.stats_color.r { Color::from_rgb(0.12, 0.28, 0.9) } else { Color::from_rgb(0.8, 0.12, 0.1) };
            l.colors[4] = c;
            l.stats.set_font_size(card_text::fit_line(&t.stats, (STATS_PLATE_SIZE.x - 10.0) * per_px, 20, 46));
            l.stats.set_outline_size(0);
            l.stats.set_modulate(c * tint);
            l.colors[0] = Color::from_rgb(0.97, 0.93, 0.85);
        } else {
            l.stats.set_font_size(44);
            l.colors[0] = Color::from_rgb(0.97, 0.93, 0.85);
            l.stats.set_modulate(t.stats_color * tint);
        }
        // Regeltext mit Manasymbolen: als Bild mit den Symbolen aus dem Bogen statt als Schrift
        let shown = l.text.is_visible();
        let (pos, font) = (l.text.get_position(), l.text.get_font_size());
        let color = l.colors[3] * tint;
        let height_px = if custom { CARD_HEIGHT * TEXT_PANEL_HEIGHT } else { TEXT_BOX_HEIGHT * CARD_WIDTH / 256.0 } / LABEL_PIXEL;
        let raw = card_text::tidy(&self.def.text);
        let rich = shown && crate::game::mana_symbols::has_symbols(&raw);
        if rich {
            l.text.set_visible(false);
        }
        self.set_rich_text(rich.then_some((raw, pos, font, height_px, color)));
        self.set_text_panel(custom);
    }

    /// Regeltext mit eingebetteten Manasymbolen (`cost_symbols::rich_text_texture`) an der Stelle
    /// der Beschriftung zeigen bzw. ausblenden (`None`).
    fn set_rich_text(&mut self, text: Option<(String, Vector3, i32, f32, Color)>) {
        let Some((raw, pos, font, height_px, color)) = text else {
            if let Some(s) = self.rich_text.as_mut() {
                s.set_visible(false);
            }
            return;
        };
        let width_px = (TEXT_WRAP_WIDTH / LABEL_PIXEL) as i32;
        let size = Vector2i::new(width_px, height_px.round() as i32);
        let Some(tex) = cost_symbols::rich_text_texture(&raw, font, size, Color::from_rgb(color.r, color.g, color.b)) else { return };
        if self.rich_text.is_none() {
            let Some(visual) = self.visual.as_mut() else { return };
            let mut sprite = godot::classes::Sprite3D::new_alloc();
            sprite.set_name("RichText");
            sprite.set_pixel_size(LABEL_PIXEL);
            sprite.set_draw_flag(godot::classes::sprite_base_3d::DrawFlags::DOUBLE_SIDED, false);
            sprite.set_draw_flag(godot::classes::sprite_base_3d::DrawFlags::SHADED, false);
            sprite.set_rotation(Vector3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0));
            visual.add_child(&sprite);
            self.rich_text = Some(sprite);
        }
        if let Some(s) = self.rich_text.as_mut() {
            s.set_texture(&tex);
            s.set_position(pos);
            s.set_visible(true);
        }
    }

    /// Feld über dem gedruckten Textfeld eines echten Kartenbilds (für eigenen Text einer Bühne,
    /// `CardDef::custom_text`), in Pergamentfarbe; nur bei Bedarf angelegt.
    fn set_text_panel(&mut self, shown: bool) {
        if self.text_panel.is_none() && shown {
            let Some(visual) = self.visual.as_mut() else { return };
            let mut mesh = PlaneMesh::new_gd();
            mesh.set_size(Vector2::new(CARD_WIDTH * TEXT_PANEL_WIDTH, CARD_HEIGHT * TEXT_PANEL_HEIGHT));
            let mut mat = StandardMaterial3D::new_gd();
            mat.set_shading_mode(ShadingMode::UNSHADED);
            mat.set_albedo(Color::from_rgb(0.94, 0.91, 0.84));
            mesh.set_material(&mat);
            let mut panel = MeshInstance3D::new_alloc();
            panel.set_name("TextPanel");
            panel.set_mesh(&mesh);
            panel.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            panel.set_position(Vector3::new(0.0, CARD_THICKNESS * 0.5 + 0.0008, (TEXT_PANEL_CENTER - 0.5) * CARD_HEIGHT));
            visual.add_child(&panel);
            self.text_panel = Some(panel);
        }
        if let Some(p) = self.text_panel.as_mut() {
            p.set_visible(shown);
        }
    }

    fn build_labels(&mut self, visual: &mut Gd<Node3D>) {
        // Positionen passend zum Layout der Platzhalter-Textur (256 × 358 px).
        let px = |x: f32, y: f32| {
            Vector3::new((x / 256.0 - 0.5) * CARD_WIDTH, CARD_THICKNESS * 0.5 + 0.0015, (y / 358.0 - 0.5) * CARD_HEIGHT)
        };
        let light = Color::from_rgb(0.97, 0.93, 0.85);
        let dark = Color::from_rgb(0.18, 0.12, 0.08);
        let labels = CardLabels {
            name: card_label(visual, px(110.0, 36.0), 36, light, 0.0),
            cost: card_label(visual, px(218.0, 36.0), 34, dark, 0.0),
            kind: card_label(visual, px(128.0, 219.0), 34, light, 0.0),
            text: card_label(visual, px(128.0, TEXT_BOX_CENTER), 30, dark, 0.52),
            stats: card_label(visual, px(206.0, 333.0), 44, Color::WHITE, 0.0),
            stats_plate: stats_plate(visual),
            badges: card_label(visual, px(128.0, 130.0), 52, Color::WHITE, 0.6),
            colors: [light, dark, light, dark, Color::WHITE, Color::WHITE],
        };
        let mut badges = labels.badges.clone();
        badges.set_outline_size(14);
        badges.set_outline_modulate(Color::from_rgba(0.05, 0.04, 0.03, 0.92));
        style_badge(&mut badges, &self.badges);
        self.labels = Some(labels);
        self.refresh_labels();
    }

    /// Kostensymbole über der Oberkante ein-/ausblenden: sichtbar in Zonen mit `show_costs`
    /// (eigene Hand), wenn die Karte offen liegt, Kosten hat und kein Land ist. Rechts oben, zur
    /// Hälfte über die Kante ragend, wie im Video „Spree und Stack“.
    fn update_cost_row(&mut self) {
        // läuft jedes Frame: erst die billigen Bedingungen, Kosten nur beim Anlegen zerlegen
        let want = self.zone.as_ref().is_some_and(|z| z.bind().shows_costs())
            && !self.def.face_down
            && self.card_kind != CardKind::Land;
        if let Some((shown, row)) = self.cost_row.as_mut() {
            if *shown == self.def.cost_text {
                // eine Reihe gibt es nur zu Kosten mit Symbolen
                if row.is_visible() != want {
                    row.set_visible(want);
                }
                return;
            }
            row.queue_free();
            self.cost_row = None;
        }
        if !want {
            return;
        }
        let text = self.def.cost_text.clone();
        let tokens = cost_symbols::cost_tokens(&text);
        if tokens.is_empty() {
            return;
        }
        let Some(visual) = self.visual.as_mut() else { return };
        let width = cost_symbols::row_width(tokens.len(), HAND_COST_SIZE);
        let mut row = cost_symbols::cost_row(&tokens, HAND_COST_SIZE);
        // eine Ebene unter den Knöpfen und Bühnen-Beschriftungen (Priorität 1/2): Kosten über
        // den Handkarten dürfen nie über einem Knopf liegen
        for child in row.get_children().iter_shared() {
            if let Ok(mut sprite) = child.clone().try_cast::<godot::classes::Sprite3D>() {
                sprite.set_render_priority(0);
            } else if let Ok(mut label) = child.try_cast::<godot::classes::Label3D>() {
                label.set_render_priority(0);
            }
        }
        // flach auf der Kartenebene (Vorderseite +Y, oben −Z), rechtsbündig über der Oberkante
        row.set_transform(Transform3D::new(
            Basis::from_axis_angle(Vector3::RIGHT, -std::f32::consts::FRAC_PI_2),
            Vector3::new(
                CARD_WIDTH * 0.5 - width * 0.5 - HAND_COST_INSET,
                CARD_THICKNESS * 0.5 + 0.004,
                -CARD_HEIGHT * 0.5 - HAND_COST_SIZE * 0.2,
            ),
        ));
        visual.add_child(&row);
        self.cost_row = Some((text, row));
    }

    pub(crate) fn set_zone(&mut self, zone: Option<Gd<Zone>>) {
        self.zone = zone;
    }

    /// Globale Ziel-Transform des `Visual` für den Hover-Zoom,
    /// oder None, wenn nicht gezoomt werden soll (verdeckt, schon groß genug, …).
    fn zoom_transform(&self, card_xf: &Transform3D) -> Option<Transform3D> {
        if !self.zoom_enabled {
            return None;
        }
        let viewport = self.base().get_viewport()?;
        let cam = viewport.get_camera_3d()?;
        let cam_xf = cam.get_global_transform();
        if cam.is_position_behind(card_xf.origin) {
            return None;
        }
        // Nur offene Karten zoomen: Vorderseite (+Y) muss zur Kamera zeigen.
        if card_xf.basis.col_b().dot(cam_xf.origin - card_xf.origin) <= 0.0 {
            return None;
        }

        let screen = viewport.get_visible_rect().size;
        let target_px = screen.y * self.zoom_screen_height;

        // Ist die Karte schon so groß im Bild (z. B. Kamera-Nahaufnahme)? Dann nicht zoomen.
        let top = cam.unproject_position(*card_xf * Vector3::new(0.0, 0.0, -CARD_HEIGHT * 0.5));
        let bottom = cam.unproject_position(*card_xf * Vector3::new(0.0, 0.0, CARD_HEIGHT * 0.5));
        if top.distance_to(bottom) >= target_px * 0.9 {
            return None;
        }

        // Bildschirmposition so begrenzen, dass die große Karte vollständig sichtbar ist.
        let half = Vector2::new(target_px * CARD_WIDTH / CARD_HEIGHT, target_px) * 0.5
            + Vector2::splat(self.zoom_margin_px);
        let p = cam.unproject_position(card_xf.origin);
        let p = Vector2::new(
            p.x.clamp(half.x, (screen.x - half.x).max(half.x)),
            p.y.clamp(half.y, (screen.y - half.y).max(half.y)),
        );

        let d = self.zoom_distance;
        let pos = cam.project_position(p, d);
        let view_h = 2.0 * d * (cam.get_fov().to_radians() * 0.5).tan();
        let s = view_h * self.zoom_screen_height / CARD_HEIGHT;

        // Karte aufrecht zur Kamera: Kartennormale (+Y) -> zur Kamera (+Z der Kamera),
        // Kartenoberkante (-Z) -> Bildschirm oben (+Y der Kamera).
        let cb = cam_xf.basis.orthonormalized();
        let basis = Basis::from_cols(cb.col_a() * s, cb.col_c() * s, -cb.col_b() * s);
        Some(Transform3D::new(basis, pos))
    }

    fn build_nodes(&mut self) {
        // Kollision: nur für Raycasts, die Karte selbst überwacht nichts.
        self.base_mut().set_collision_layer(CARD_LAYER);
        self.base_mut().set_collision_mask(0);
        self.base_mut().set_monitoring(false);
        self.base_mut().set_monitorable(false);

        let mut shape = BoxShape3D::new_gd();
        shape.set_size(Vector3::new(CARD_WIDTH, CARD_THICKNESS * 4.0, CARD_HEIGHT));
        let shape_ref = shape.clone();
        let mut collision = CollisionShape3D::new_alloc();
        collision.set_shape(&shape);
        self.base_mut().add_child(&collision);

        let mut visual = Node3D::new_alloc();
        visual.set_name("Visual");

        // Kartenkörper
        let mut body_mesh = BoxMesh::new_gd();
        body_mesh.set_size(Vector3::new(CARD_WIDTH, CARD_THICKNESS, CARD_HEIGHT));
        let mut body_mat = StandardMaterial3D::new_gd();
        body_mat.set_albedo(Color::from_rgb(0.92, 0.9, 0.85));
        body_mat.set_roughness(0.8);
        body_mesh.set_material(&body_mat);
        let mut body = MeshInstance3D::new_alloc();
        body.set_name("Body");
        body.set_mesh(&body_mesh);
        visual.add_child(&body);

        // Vorderseite (+Y)
        let mut face_mat = StandardMaterial3D::new_gd();
        face_mat.set_roughness(0.55);
        face_mat.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS_ANISOTROPIC);
        apply_texture(&mut face_mat, self.face_texture.as_ref(), Color::from_rgb(0.9, 0.9, 0.9));
        let mut face_mesh = PlaneMesh::new_gd();
        face_mesh.set_size(Vector2::new(CARD_WIDTH, CARD_HEIGHT));
        face_mesh.set_material(&face_mat);
        let mut face = MeshInstance3D::new_alloc();
        face.set_name("Face");
        face.set_mesh(&face_mesh);
        face.set_position(Vector3::new(0.0, CARD_THICKNESS * 0.5 + 0.0005, 0.0));
        visual.add_child(&face);

        // Rückseite (-Y)
        let mut back_mat = StandardMaterial3D::new_gd();
        back_mat.set_roughness(0.55);
        back_mat.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS_ANISOTROPIC);
        apply_texture(&mut back_mat, self.back_texture.as_ref(), Color::from_rgb(0.15, 0.2, 0.45));
        let mut back_mesh = PlaneMesh::new_gd();
        back_mesh.set_size(Vector2::new(CARD_WIDTH, CARD_HEIGHT));
        back_mesh.set_flip_faces(true);
        back_mesh.set_material(&back_mat);
        let mut back = MeshInstance3D::new_alloc();
        back.set_name("Back");
        back.set_mesh(&back_mesh);
        back.set_position(Vector3::new(0.0, -(CARD_THICKNESS * 0.5 + 0.0005), 0.0));
        visual.add_child(&back);

        self.base_mut().add_child(&visual);
        self.visual = Some(visual.clone());
        // Kampfmarkierung: flackernde Aura rund um die Karte (sonst unsichtbar)
        let glow_shader = crate::table::shared::shader(GLOW_SHADER);
        let mut marker_mat = ShaderMaterial::new_gd();
        marker_mat.set_shader(&glow_shader);
        let quad = Vector2::new(CARD_WIDTH, CARD_HEIGHT) + Vector2::splat(2.0 * GLOW_MARGIN);
        marker_mat.set_shader_parameter("quad_size", &quad.to_variant());
        marker_mat.set_shader_parameter("card_half", &(Vector2::new(CARD_WIDTH, CARD_HEIGHT) * 0.5).to_variant());
        let mut marker_mesh = PlaneMesh::new_gd();
        marker_mesh.set_size(quad);
        marker_mesh.set_material(&marker_mat);
        let mut marker = MeshInstance3D::new_alloc();
        marker.set_name("Marker");
        marker.set_mesh(&marker_mesh);
        marker.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        marker.set_visible(false);
        // Zwischen Tischplatte und Kartenbild: die Karte verdeckt die Mitte, nur der Schein ragt
        // heraus. Nicht tiefer als die Unterseite der Karte, sonst läge er bei Karten direkt auf
        // dem Tisch unter der Tischplatte und wäre unsichtbar (Blocker ohne Rahmen).
        marker.set_position(Vector3::new(0.0, -CARD_THICKNESS * 0.5 + 0.0025, 0.0));
        visual.add_child(&marker);
        self.marker_mesh = Some(marker);
        self.marker_material = Some(marker_mat);

        // Angriffsspitze: rote Dachform über der Oberkante (zeigt zum Ziel), nur bei Angreifern
        let banner_shader = crate::table::shared::shader(BANNER_SHADER);
        let mut banner_mat = ShaderMaterial::new_gd();
        banner_mat.set_shader(&banner_shader);
        let mut banner_mesh = PlaneMesh::new_gd();
        banner_mesh.set_size(BANNER_SIZE);
        banner_mesh.set_material(&banner_mat);
        let mut banner = MeshInstance3D::new_alloc();
        banner.set_name("AttackBanner");
        banner.set_mesh(&banner_mesh);
        banner.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        banner.set_position(Vector3::new(0.0, CARD_THICKNESS, -CARD_HEIGHT * 0.5 - BANNER_SIZE.y * 0.5 + 0.02));
        banner.set_visible(self.advanced);
        visual.add_child(&banner);
        self.attack_banner = Some(banner);
        self.banner_material = Some(banner_mat);

        // Blockschweif: Bogen aus Licht über der Kante zum Angreifer, nur bei Blockern
        let trail_shader = crate::table::shared::shader(&TRAIL_SHADER.replace("%NOISE%", PERLIN_FBM));
        let mut trail_mat = ShaderMaterial::new_gd();
        trail_mat.set_shader(&trail_shader);
        trail_mat.set_shader_parameter("seed", &(self.base().instance_id().to_i64().rem_euclid(97) as f32 * 0.37).to_variant());
        let mut trail_mesh = PlaneMesh::new_gd();
        trail_mesh.set_size(Vector2::new(CARD_WIDTH + 2.0 * TRAIL_MARGIN, TRAIL_HEIGHT));
        trail_mesh.set_material(&trail_mat);
        let mut trail = MeshInstance3D::new_alloc();
        trail.set_name("BlockTrail");
        trail.set_mesh(&trail_mesh);
        trail.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        trail.set_visible(false);
        visual.add_child(&trail);
        self.trail_mesh = Some(trail);
        self.trail_material = Some(trail_mat);
        self.place_banner();

        // Tapp-Symbol: knapp über der Vorderseite, mittig im Bildfeld, nur wenn getappt sichtbar
        let mut icon_mat = StandardMaterial3D::new_gd();
        icon_mat.set_shading_mode(ShadingMode::UNSHADED);
        icon_mat.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA);
        icon_mat.set_texture(TextureParam::ALBEDO, &super::shared::texture("tap_icon", super::textures::tap_icon));
        icon_mat.set_texture_filter(TextureFilter::LINEAR_WITH_MIPMAPS);
        let mut icon_mesh = PlaneMesh::new_gd();
        icon_mesh.set_size(Vector2::splat(TAP_ICON_SIZE));
        icon_mesh.set_material(&icon_mat);
        let mut icon = MeshInstance3D::new_alloc();
        icon.set_name("TapIcon");
        icon.set_mesh(&icon_mesh);
        icon.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        icon.set_position(Vector3::new(0.0, CARD_THICKNESS * 0.5 + 0.003, -CARD_HEIGHT * 0.12));
        icon.set_visible(false);
        visual.add_child(&icon);
        self.tap_icon = Some(icon);

        self.build_labels(&mut visual);
        self.shape_refs = Some(ShapeRefs { body: body_mesh, face: face_mesh, back: back_mesh, shape: shape_ref, marker: marker_mesh });
        self.meshes = vec![body, face, back];
        self.face_material = Some(face_mat);
        self.back_material = Some(back_mat);
        self.apply_tapped_look();
        if self.tile_shown {
            self.apply_shape();
        }
        self.apply_brightness();
    }
}

/// Liegen zwei Transformationen so nah beieinander, dass der Unterschied unsichtbar ist
/// (Kartenbild am Ziel angekommen)?
fn transforms_close(a: &Transform3D, b: &Transform3D) -> bool {
    const EPS: f32 = 1e-4;
    let near = |u: Vector3, v: Vector3| {
        let d = u - v;
        d.x.abs() <= EPS && d.y.abs() <= EPS && d.z.abs() <= EPS
    };
    near(a.origin, b.origin) && near(a.basis.col_a(), b.basis.col_a()) && near(a.basis.col_b(), b.basis.col_b()) && near(a.basis.col_c(), b.basis.col_c())
}

fn apply_texture(mat: &mut Gd<StandardMaterial3D>, tex: Option<&Gd<Texture2D>>, fallback: Color) {
    match tex {
        Some(tex) => {
            mat.set_texture(TextureParam::ALBEDO, tex);
            mat.set_albedo(Color::WHITE);
        }
        None => {
            mat.set_texture(TextureParam::ALBEDO, Gd::<Texture2D>::null_arg());
            mat.set_albedo(fallback);
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum CardMarker {
    #[default]
    None,
    Attacking,
    Blocking,
    Selected,
}

/// Beschriftung auf der Vorderseite (Label3D, liegen flach auf der Karte).
/// Formteile, die sich mit der Kachel-Form ändern.
struct ShapeRefs {
    body: Gd<BoxMesh>,
    face: Gd<PlaneMesh>,
    back: Gd<PlaneMesh>,
    shape: Gd<BoxShape3D>,
    marker: Gd<PlaneMesh>,
}

struct CardLabels {
    name: Gd<Label3D>,
    cost: Gd<Label3D>,
    kind: Gd<Label3D>,
    text: Gd<Label3D>,
    stats: Gd<Label3D>,
    /// Eigenes Wertefeld über dem gedruckten (echtes Kartenbild, Werte verändert): deckt das
    /// gedruckte Feld ganz ab, die Werte stehen mittig darauf.
    stats_plate: Gd<MeshInstance3D>,
    /// Plaketten über dem Bildfeld: Marken, „Spielstein“ (siehe [`Card3D::set_badges`]).
    badges: Gd<Label3D>,
    /// Grundfarbe je Beschriftung (Reihenfolge wie [`CardLabels::all`]); das Ausgrauen
    /// getappter Karten multipliziert sie.
    colors: [Color; 6],
}

impl CardLabels {
    fn all(&mut self) -> [&mut Gd<Label3D>; 6] {
        [&mut self.name, &mut self.cost, &mut self.kind, &mut self.text, &mut self.stats, &mut self.badges]
    }
}

/// Eigenes Wertefeld auf echten Kartenbildern: Mitte und Größe in Pixeln der Kartentextur
/// (256 × 358) – gemessen an Scryfall-Bildern alter und neuer Rahmen (gedrucktes Feld etwa
/// x 190…242, y 317…340), etwas größer, damit es das gedruckte Feld ganz abdeckt.
const STATS_PLATE_CENTER: Vector2 = Vector2::new(216.0, 328.5);
const STATS_PLATE_SIZE: Vector2 = Vector2::new(56.0, 27.0);

thread_local! {
    /// Textur des eigenen Wertefelds (einmal gezeichnet).
    static STATS_PLATE_TEXTURE: std::cell::RefCell<Option<Gd<Texture2D>>> = const { std::cell::RefCell::new(None) };
}

/// Zwischenspeicher des Wertefeld-Bilds leeren (Tisch wird abgebaut; sonst bliebe die Textur
/// beim Beenden liegen).
pub fn clear_stats_plate_cache() {
    STATS_PLATE_TEXTURE.with(|t| *t.borrow_mut() = None);
}

/// Eigenes Wertefeld (Pergament mit dunklem Rand und abgerundeten Ecken wie das gedruckte),
/// flach über der Vorderseite; anfangs unsichtbar.
fn stats_plate(parent: &mut Gd<Node3D>) -> Gd<MeshInstance3D> {
    let tex = STATS_PLATE_TEXTURE.with(|t| t.borrow_mut().get_or_insert_with(stats_plate_texture).clone());
    let mut mat = StandardMaterial3D::new_gd();
    mat.set_texture(godot::classes::base_material_3d::TextureParam::ALBEDO, &tex);
    mat.set_transparency(godot::classes::base_material_3d::Transparency::ALPHA_SCISSOR);
    // unbeleuchtet wie das gedruckte Feld im Kartenbild (sonst dunkler als die Karte)
    mat.set_shading_mode(godot::classes::base_material_3d::ShadingMode::UNSHADED);
    let mut mesh = PlaneMesh::new_gd();
    mesh.set_size(Vector2::new(STATS_PLATE_SIZE.x / 256.0 * CARD_WIDTH, STATS_PLATE_SIZE.y / 358.0 * CARD_HEIGHT));
    mesh.set_material(&mat);
    let mut mi = MeshInstance3D::new_alloc();
    mi.set_mesh(&mesh);
    mi.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    mi.set_position(Vector3::new(
        (STATS_PLATE_CENTER.x / 256.0 - 0.5) * CARD_WIDTH,
        CARD_THICKNESS * 0.5 + 0.0012,
        (STATS_PLATE_CENTER.y / 358.0 - 0.5) * CARD_HEIGHT,
    ));
    mi.set_visible(false);
    parent.add_child(&mi);
    mi
}

/// Bild des Wertefelds: abgerundetes Pergament-Rechteck, oben heller, mit dunklem Rand.
fn stats_plate_texture() -> Gd<Texture2D> {
    use godot::classes::image::Format;
    use godot::classes::{Image, ImageTexture};
    let (w, h) = (224i32, 108i32);
    let mut img = Image::create_empty(w, h, false, Format::RGBA8).expect("Bild");
    let r = 30.0f32;
    let border = 7.0f32;
    let sdf = |x: f32, y: f32| {
        // Abstand zum Rand des abgerundeten Rechtecks (negativ = innen)
        let (hx, hy) = (w as f32 * 0.5 - r - 1.0, h as f32 * 0.5 - r - 1.0);
        let (qx, qy) = ((x - w as f32 * 0.5).abs() - hx, (y - h as f32 * 0.5).abs() - hy);
        (qx.max(0.0).hypot(qy.max(0.0)) + qx.max(qy).min(0.0)) - r
    };
    let edge = Color::from_rgb(0.33, 0.24, 0.17);
    for y in 0..h {
        let t = y as f32 / h as f32;
        let fill = Color::from_rgb(0.96, 0.9, 0.82).lerp(Color::from_rgb(0.86, 0.76, 0.66), t as f64);
        for x in 0..w {
            let d = sdf(x as f32 + 0.5, y as f32 + 0.5);
            let c = if d > 0.0 {
                Color::from_rgba(0.0, 0.0, 0.0, 0.0)
            } else if d > -border {
                edge
            } else {
                fill
            };
            img.set_pixel(x, y, c);
        }
    }
    ImageTexture::create_from_image(&img).expect("Textur").upcast()
}

/// Textfeld für den Regeltext: Mitte und Höhe in Pixeln der Kartentextur (unten bleibt das
/// Wertefeld frei).
const TEXT_BOX_CENTER: f32 = 276.0;
const TEXT_BOX_HEIGHT: f32 = 72.0;
/// Umbruchbreite des Regeltexts (m), wie die Beschriftung in `build_labels`.
const TEXT_WRAP_WIDTH: f32 = 0.52;
/// Textfeld eines echten Kartenbilds (Scryfall, moderner Rahmen): Mitte (Anteil der Höhe von
/// oben), Breite und Höhe (Anteile der Karte) des Feldes für eigenen Text.
const TEXT_PANEL_CENTER: f32 = 0.765;
const TEXT_PANEL_WIDTH: f32 = 0.86;
const TEXT_PANEL_HEIGHT: f32 = 0.27;

/// Größe eines Schriftpunkts der Kartenbeschriftung (m).
const LABEL_PIXEL: f32 = 0.0008;

/// Plakette in der Kartenmitte setzen. Kurze Zahlen (Reihenfolge „1“, „2“ …, Wiederholung
/// „×2“) stehen groß und fett, damit man sie nach dem Anklicken sofort sieht; längere Texte
/// („Spielstein“, „aus dem Friedhof“ …) normal.
fn style_badge(l: &mut Gd<Label3D>, text: &str) {
    let number = !text.is_empty() && text.chars().count() <= 3 && text.chars().all(|c| c.is_ascii_digit() || c == '×');
    if number {
        let mut font = godot::classes::SystemFont::new_gd();
        font.set_font_names(&PackedStringArray::from(&[GString::from("Noto Sans"), GString::from("DejaVu Sans"), GString::from("sans-serif")]));
        font.set_font_weight(800);
        l.set_font(&font);
        l.set_font_size(BADGE_NUMBER_FONT);
        l.set_outline_size(BADGE_NUMBER_FONT / 6);
    } else {
        l.set_font(Gd::<godot::classes::Font>::null_arg());
        l.set_font_size(52);
        l.set_outline_size(14);
    }
    l.set_text(text);
}

/// Schriftgröße kurzer Zahlen-Plaketten ([`style_badge`]).
const BADGE_NUMBER_FONT: i32 = 240;

fn card_label(parent: &mut Gd<Node3D>, pos: Vector3, font_size: i32, color: Color, wrap_width: f32) -> Gd<Label3D> {
    const PIXEL: f32 = LABEL_PIXEL;
    let mut l = Label3D::new_alloc();
    l.set_pixel_size(PIXEL);
    l.set_font_size(font_size);
    l.set_modulate(color);
    l.set_outline_size(0);
    l.set_horizontal_alignment(HorizontalAlignment::CENTER);
    l.set_draw_flag(DrawFlags::DOUBLE_SIDED, false);
    l.set_draw_flag(DrawFlags::SHADED, true);
    if wrap_width > 0.0 {
        l.set_autowrap_mode(godot::classes::text_server::AutowrapMode::WORD_SMART);
        l.set_width(wrap_width / PIXEL);
    }
    // Flach auf die Vorderseite legen: Label schaut nach +Y, Textoberkante nach -Z.
    l.set_rotation(Vector3::new(-std::f32::consts::FRAC_PI_2, 0.0, 0.0));
    l.set_position(pos);
    l.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    parent.add_child(&l);
    l
}

/// Texte der Kartenvorderseite (siehe [`Card3D::face_texts`]). Positionen `*_x` in
/// Pixeln der Kartentextur (256 breit); die y-Positionen sind für alle Karten gleich.
pub struct FaceTexts {
    pub name: String,
    pub name_x: f32,
    /// Freie Breite für den Namen (Pixel der Kartentextur).
    pub name_width: f32,
    /// Generische Kosten als Zahl ("" = keine), Mitte des grauen Kostensymbols bei `cost_x`.
    pub cost: String,
    pub cost_x: f32,
    pub kind: String,
    pub body: String,
    pub stats: String,
    pub stats_color: Color,
    /// Echtes Kartenbild statt Platzhalter: Name, Kosten, Typzeile und Text stehen im Bild.
    pub printed: bool,
    /// Eigener Text einer Bühne (`CardDef::custom_text`).
    pub custom_text: bool,
    /// Karte zeigt ihre Kachel-Textur (Name und Werte auf die Kachel).
    pub tile_textured: bool,
}

/// Verstärkung des Aufleuchtens `t` Sekunden nach [`Card3D::flash`].
fn flash_gain(t: f32) -> f32 {
    for w in FLASH_KEYS.windows(2) {
        let ((t0, v0), (t1, v1)) = (w[0], w[1]);
        if t <= t1 {
            let u = ((t - t0) / (t1 - t0)).clamp(0.0, 1.0);
            return v0 + (v1 - v0) * u * u * (3.0 - 2.0 * u);
        }
    }
    1.0
}
