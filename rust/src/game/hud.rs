//! TurnHud: Bildschirm-Overlay für den Rundenablauf.
//!
//! - unten rechts über dem Phasen-Knopf: Zeitstrahl der Phasen ([`PhaseTimeline`])
//! - Bildmitte: nur die Meldung zum Spielende (keine Hinweistexte)
//! - Medaillon ([`SeatBadge`]) mit Leben (ohne Text daneben; ausgegraut, solange eine 3D-Bühne
//!   wie der Mulligan offen ist): folgt je einem 3D-Ankerpunkt
//!   (Spieler: direkt über der Hand, Gegner: direkt unter dessen Hand); ohne Anker oben links
//!   (Gegner) bzw. unten links (Spieler)
//! - unter der Ressourcen-Reihe des Spielers (3D-Anker, sonst rechts neben seinem Medaillon):
//!   Manapool als runde Symbole ([`ManaPoolView`])
//! - linker Bildrand: Spielername, darüber die Rundenzahl mit Sanduhr ([`HourglassIcon`]) –
//!   Gegner oben links, Spieler unten links; der Name der Seite am Zug ist golden
//! - unten rechts: Phasen-Knopf „Passen“ (auch Leertaste; im Kampf „Alle greifen an“ /
//!   „2 Angreifer“, „Kein Block“ / „1 Blocker“) und darunter klein „»»“ (Sofort beenden) – beide
//!   als orange animierte [`Button3D`] in der 3D-Szene, an die Bildschirmstelle geheftet; im
//!   Kampf darüber ein zweiter Knopf („Kein Angriff“ blau, „Angriff abbrechen“ grau), für den
//!   der Zeitstrahl nach oben rückt
//! - Bildschirmmitte: Auswahlfenster (z. B. Mulligan)
//!
//! Das HUD liest den Zustand jedes Frame vom [`ForgeTable`] (keine Signal-Kopplung nötig).
//! Alle 2D-Flächen lassen Mausklicks durch; die 3D-Knöpfe klickt der `TableController` an.

use godot::classes::control::{GrowDirection, MouseFilter};
use godot::classes::text_server::AutowrapMode;
use godot::classes::{
    Button, CanvasLayer, Control, ICanvasLayer, InputEvent, InputEventKey, Label, PanelContainer, RichTextLabel,
    StyleBoxFlat, VBoxContainer,
};
use godot::builtin::Side;
use godot::global::Key;
use godot::classes::object::ConnectFlags;
use godot::prelude::*;

use super::card_cutout::CardCutout;
use super::hourglass::HourglassIcon;
use super::mana_view::ManaPoolView;
use super::phase_timeline::PhaseTimeline;
use super::seat_badge::SeatBadge;
use super::choice_panel::ChoicePanel;
pub use super::choice_panel::HudChoice;
use crate::forge::table::ForgeTable;
use crate::table::button3d::{Button3D, ButtonStyle, COMPACT_BODY, WIDE_BODY};
use crate::table::layers::{HUD_CANVAS_LAYER, SEAT_CANVAS_LAYER};
use crate::table::table_controller::TableController;

const GOLD: Color = Color::from_rgb(0.95, 0.78, 0.4);
const PARCHMENT: Color = Color::from_rgb(0.93, 0.87, 0.74);
const DIM: Color = Color::from_rgba(0.93, 0.87, 0.74, 0.45);
/// Seitenfarben (passen zu den Bannern der Arena).
const PLAYER_ACCENT: Color = Color::from_rgb(0.25, 0.55, 0.95);
const OPPONENT_ACCENT: Color = Color::from_rgb(0.9, 0.25, 0.22);
/// Ausgegraut und fast durchsichtig, solange eine 3D-Bühne offen ist (Medaillons, Namen,
/// Manapool) – sie liegen als 2D-Anzeige über der Bühne und dürfen Texte und Knöpfe nicht verdecken.
const STAGE_TINT: Color = Color::from_rgba(0.42, 0.42, 0.46, 0.22);
/// Phasen-Knopf und „Sofort beenden“ ([Button3D], unten rechts): Breite der Knopf-Fläche in
/// Pixeln, Rand zum Bildschirm und Abstand zwischen den Knöpfen.
const MAIN_FACE_PX: f32 = 225.0;
/// „»»“: kurzer Knopf, nur so groß wie die Symbole (Höhe der Knopf-Fläche in Pixeln).
const END_FACE_HEIGHT_PX: f32 = 34.0;
const BUTTON_MARGIN: f32 = 14.0;
const BUTTON_GAP_PX: f32 = 8.0;
/// Abstand des Phasen-Zeitstrahls über dem Phasen-Knopf.
const TIMELINE_GAP: f32 = 10.0;
/// Zeitstrahl und Hinweis rücken weich nach oben bzw. unten, wenn der zweite Knopf über dem
/// Phasen-Knopf erscheint bzw. verschwindet (exponentiell, ≈ 0,2 s wie das Erscheinen des Knopfes).
const STACK_LIFT_SPEED: f32 = 14.0;

/// Bildschirm-Overlay für den Rundenablauf: Zeitstrahl der Phasen ([PhaseTimeline], unten rechts über dem Hauptbutton), Spielende-Meldung in der Bildmitte,
/// je ein [SeatBadge]-Medaillon mit Leben (an [member player_hero_anchor] / [member opponent_hero_anchor],
/// sonst links oben / unten), am linken Bildrand Spielername und darüber Rundenzahl mit Sanduhr
/// ([HourglassIcon]; Gegner oben, Spieler unten), unten rechts der Phasen-Knopf „Passen“ (auch Leertaste; im Kampf
/// „Alle greifen an“ / „2 Angreifer“, „Kein Block“ / „1 Blocker“) und darunter „»»“ (Sofort beenden) (beide
/// orange animierte [Button3D]), im Kampf darüber ein zweiter Knopf („Kein Angriff“, „Angriff abbrechen“,
/// „Blocks abbrechen“), und ein Auswahlfenster (z. B. Mulligan).
/// Liest den Zustand jedes Frame vom [ForgeTable] (Spiel mit der Forge-Regel-Engine).
#[derive(GodotClass)]
#[class(base=CanvasLayer)]
pub struct TurnHud {
    /// Das Forge-Spiel ([ForgeTable]), dessen Zustand angezeigt wird.
    #[export]
    forge_table: Option<Gd<ForgeTable>>,
    /// 3D-Punkt, an dem das Spieler-Medaillon sitzt (Unterkante mittig, z. B. direkt über der Hand).
    /// Leer = Medaillon unten links. Gezoomte oder gezogene Karten liegen sichtbar über dem
    /// Medaillon (Aussparung per [CardCutout]).
    #[export]
    player_hero_anchor: Option<Gd<Node3D>>,
    /// 3D-Punkt, an dem das Gegner-Medaillon sitzt (Oberkante mittig, z. B. direkt unter dessen Hand).
    /// Leer = Medaillon oben links.
    #[export]
    opponent_hero_anchor: Option<Gd<Node3D>>,
    /// 3D-Punkt, an dem die Mana-Symbole des Spielers enden (rechte obere Ecke, z. B. direkt unter
    /// der Ressourcen-Reihe). Leer = rechts neben dem Spieler-Medaillon.
    #[export]
    player_mana_anchor: Option<Gd<Node3D>>,

    timeline: Option<Gd<PhaseTimeline>>,
    /// Linker Bildrand: Name und Rundenzahl von Gegner (oben) und Spieler (unten).
    opponent_name: Option<Gd<Label>>,
    player_name: Option<Gd<Label>>,
    opponent_turn: Option<Gd<Label>>,
    player_turn: Option<Gd<Label>>,
    opponent_badge: Option<Gd<SeatBadge>>,
    player_badge: Option<Gd<SeatBadge>>,
    /// Zeilen (Medaillon + Text) von Gegner und Spieler, für die Positionierung am Anker.
    opponent_row: Option<Gd<Control>>,
    player_row: Option<Gd<Control>>,
    /// Manapool des Spielers als runde Symbole, rechts neben seinem Medaillon.
    player_mana: Option<Gd<ManaPoolView>>,
    /// Masken über den Medaillon-Zeilen (Gegner, Spieler): sparen die gehoverte/gezogene Karte aus.
    opponent_mask: Option<Gd<CardCutout>>,
    player_mask: Option<Gd<CardCutout>>,
    /// Meldung „Spielende – X gewinnt!“ in der Bildmitte (sonst leer).
    game_over_label: Option<Gd<Label>>,
    /// Phasen-Knopf („Passen“, „Alle greifen an“ …) und „Sofort beenden“ als [Button3D].
    button: Option<Gd<Button3D>>,
    end_turn_button: Option<Gd<Button3D>>,
    /// Zweiter Knopf über dem Phasen-Knopf (im Kampf „Kein Angriff“ / „Angriff abbrechen“);
    /// entsteht, wenn der [ForgeTable] einen anbietet, und verschwindet animiert.
    secondary_button: Option<Gd<Button3D>>,
    /// Bildschirmhöhe (Pixel, von unten) der Mitte des zweiten Knopfes und des Zeitstrahls
    /// (Unterkante) ohne zweiten Knopf; Höhe, um die der Zeitstrahl für ihn nach oben rückt.
    secondary_y: f32,
    timeline_y: f32,
    secondary_lift: f32,
    /// Aktuell gerückt (0 = kein zweiter Knopf, `secondary_lift` = ganz oben).
    stack_lift: f32,
    /// Hinweis der Engine über dem Phasen-Zeitstrahl (z. B. ungültiger Angriff).
    button_hint: Option<Gd<RichTextLabel>>,
    /// Zuletzt gesetzter Hinweistext (nur bei Änderung neu aufbauen).
    shown_hint: String,
    /// Elternknoten für die Vorschau (Vollbild-Root des HUD).
    root: Option<Gd<Control>>,
    /// Auswahlfenster in der Bildschirmmitte (alle allgemeinen Forge-Entscheidungen).
    choice: Option<ChoicePanel>,

    base: Base<CanvasLayer>,
}

#[godot_api]
impl ICanvasLayer for TurnHud {
    fn init(base: Base<CanvasLayer>) -> Self {
        Self {
            forge_table: None,
            timeline: None,
            opponent_name: None,
            player_name: None,
            opponent_turn: None,
            player_turn: None,
            opponent_badge: None,
            player_badge: None,
            opponent_row: None,
            player_row: None,
            player_mana: None,
            opponent_mask: None,
            player_mask: None,
            player_hero_anchor: None,
            opponent_hero_anchor: None,
            player_mana_anchor: None,
            game_over_label: None,
            button: None,
            end_turn_button: None,
            secondary_button: None,
            secondary_y: 0.0,
            timeline_y: 0.0,
            secondary_lift: 0.0,
            stack_lift: 0.0,
            button_hint: None,
            shown_hint: String::new(),
            root: None,
            choice: None,
            base,
        }
    }

    fn ready(&mut self) {
        self.build();
    }

    fn process(&mut self, delta: f64) {
        self.refresh();
        self.lift_stack(delta as f32);
    }

    fn unhandled_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(key) = event.try_cast::<InputEventKey>() {
            if key.is_pressed() && !key.is_echo() && key.get_keycode() == Key::SPACE {
                self.on_next_pressed();
            }
        }
    }
}

#[godot_api]
impl TurnHud {
    /// Option `index` im Auswahlfenster angeklickt (eine Option / Ja-Nein: Antwort senden;
    /// Mehrfachauswahl: an-/abwählen).
    #[func]
    fn on_choice_option(&mut self, index: i64) {
        let answer = self.choice.as_mut().and_then(|c| c.on_option(index.max(0) as usize));
        if let Some(answer) = answer {
            self.submit(answer);
        }
    }

    /// Knopf „Fertig“ / „OK“ im Auswahlfenster (Mehrfachauswahl, Zahl, Text).
    #[func]
    fn on_choice_done(&mut self) {
        let answer = self.choice.as_ref().and_then(|c| c.on_done());
        if let Some(answer) = answer {
            self.submit(answer);
        }
    }




    /// Mana-Symbol im Manapool angeklickt → an den [ForgeTable] (beim Bezahlen einsetzen).
    #[func]
    fn on_pool_clicked(&mut self, mana: i64) {
        if let Some(mut ft) = self.forge_table.clone() {
            ft.bind_mut().pool_clicked(mana);
        }
    }

    /// Mitte des Medaillons von Sitzplatz `seat` auf dem Bildschirm (Pixel); (−1, −1), wenn es
    /// (noch) keins gibt. Für 3D-Objekte, die sich am Medaillon ausrichten (Spielsteine der
    /// Kennzeichen, Knöpfe beim Verteilen).
    #[func]
    pub fn badge_center(&self, seat: i64) -> Vector2 {
        let badge = if seat == 0 { self.player_badge.as_ref() } else { self.opponent_badge.as_ref() };
        badge
            .filter(|b| b.is_instance_valid() && b.is_visible_in_tree())
            .map(|b| {
                let r = b.get_global_rect();
                r.position + r.size * 0.5
            })
            .unwrap_or(Vector2::new(-1.0, -1.0))
    }

    /// Ein Medaillon (Spieler als Ziel) wurde angeklickt: an den [ForgeTable] weitergeben.
    #[func]
    fn on_badge_clicked(&mut self, seat: i64) {
        if let Some(ft) = self.forge_table.as_mut() {
            ft.bind_mut().player_clicked(seat);
        }
    }

    /// Suchfeld des Auswahlfensters geändert: Optionen filtern.
    #[func]
    fn on_choice_filter_changed(&mut self, text: GString) {
        if let Some(c) = self.choice.as_mut() {
            c.filter(&text.to_string());
        }
    }

    /// Knopf „Abbrechen“ im Auswahlfenster: Antwort null.
    #[func]
    fn on_choice_cancel(&mut self) {
        self.submit(Variant::nil());
    }

    /// Phasen-Knopf bzw. Leertaste: ruft [method ForgeTable.pass_priority] auf (passen bzw. im
    /// Kampf Angreifer/Blocker bestätigen).
    #[func]
    fn on_next_pressed(&mut self) {
        if let Some(ft) = self.forge_table.as_mut() {
            ft.bind_mut().pass_priority();
        }
    }

    /// Zweiter Knopf über dem Phasen-Knopf: ruft [method ForgeTable.press_secondary] auf
    /// („Kein Angriff“, „Angriff abbrechen“, „Blocks abbrechen“).
    #[func]
    fn on_secondary_pressed(&mut self) {
        if let Some(ft) = self.forge_table.as_mut() {
            ft.bind_mut().press_secondary();
        }
    }

    /// Knopf „Sofort beenden“: ruft [method ForgeTable.end_turn] auf.
    #[func]
    fn on_end_turn_pressed(&mut self) {
        if let Some(ft) = self.forge_table.as_mut() {
            ft.bind_mut().end_turn();
        }
    }
}

impl TurnHud {
    /// Antwort auf die offene Entscheidung an den [ForgeTable] geben.
    fn submit(&mut self, answer: Variant) {
        if let Some(ft) = self.forge_table.as_mut() {
            ft.bind_mut().submit_choice(answer);
        }
    }

    fn build(&mut self) {
        // Das HUD liegt vor dem 3D-Vordergrund; Medaillons, Namen und Manapool liegen auf einer
        // eigenen Bildebene dahinter (über der Kulisse, unter Karten, Bühnen und Knöpfen)
        self.base_mut().set_layer(HUD_CANVAS_LAYER);
        let mut root = Control::new_alloc();
        root.set_name("Root");
        root.set_anchors_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        root.set_mouse_filter(MouseFilter::IGNORE);
        let mut seat_layer = CanvasLayer::new_alloc();
        seat_layer.set_name("SeatLayer");
        seat_layer.set_layer(SEAT_CANVAS_LAYER);
        let mut seat_root = Control::new_alloc();
        seat_root.set_name("SeatRoot");
        seat_root.set_anchors_preset(godot::classes::control::LayoutPreset::FULL_RECT);
        seat_root.set_mouse_filter(MouseFilter::IGNORE);
        seat_layer.add_child(&seat_root);

        // Unten rechts übereinander: Zeitstrahl der Phasen, Phasen-Knopf, „Sofort beenden“
        let end_w = Button3D::face_width_for_height(END_FACE_HEIGHT_PX, COMPACT_BODY);
        let end_h = END_FACE_HEIGHT_PX;
        let main_h = Button3D::face_height_for_face(MAIN_FACE_PX, WIDE_BODY);
        let end_y = -BUTTON_MARGIN - end_h * 0.5;
        let main_y = -BUTTON_MARGIN - end_h - BUTTON_GAP_PX - main_h * 0.5;
        let main_top = -BUTTON_MARGIN - end_h - BUTTON_GAP_PX - main_h;
        // zweiter Knopf (im Kampf) direkt über dem Phasen-Knopf, gleich groß
        self.secondary_y = main_y - main_h - BUTTON_GAP_PX;
        self.secondary_lift = main_h + BUTTON_GAP_PX;
        self.timeline_y = main_top - TIMELINE_GAP;
        let mut timeline = PhaseTimeline::new_alloc();
        place(&mut timeline, (1.0, 1.0), (-BUTTON_MARGIN, self.timeline_y), GrowDirection::BEGIN, GrowDirection::BEGIN);
        root.add_child(&timeline);
        // Hinweis mit eingebetteten Manasymbolen („… {2}{R}{R}“): RichTextLabel
        let mut button_hint = RichTextLabel::new_alloc();
        button_hint.set_fit_content(true);
        button_hint.set_scroll_active(false);
        button_hint.set_horizontal_alignment(godot::global::HorizontalAlignment::RIGHT);
        button_hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        button_hint.set_custom_minimum_size(Vector2::new(360.0, 0.0));
        button_hint.set_mouse_filter(MouseFilter::IGNORE);
        button_hint.add_theme_font_size_override("normal_font_size", 15);
        button_hint.add_theme_color_override("default_color", GOLD);
        button_hint.add_theme_color_override("font_outline_color", Color::from_rgba(0.0, 0.0, 0.0, 0.8));
        button_hint.add_theme_constant_override("outline_size", 5);
        place(&mut button_hint, (1.0, 1.0), (-BUTTON_MARGIN, self.timeline_y - 66.0), GrowDirection::BEGIN, GrowDirection::BEGIN);
        root.add_child(&button_hint);

        // Bildmitte: nur die Meldung zum Spielende
        let mut hint = label(26, GOLD);
        hint.set_horizontal_alignment(godot::global::HorizontalAlignment::CENTER);
        hint.set_custom_minimum_size(Vector2::new(520.0, 0.0));
        hint.set_autowrap_mode(AutowrapMode::WORD_SMART);
        hint.add_theme_constant_override("outline_size", 6);
        place(&mut hint, (0.5, 0.5), (0.0, 0.0), GrowDirection::BOTH, GrowDirection::BOTH);
        root.add_child(&hint);

        // Linker Bildrand: Rundenzahl mit Sanduhr, darunter der Name (Gegner oben, Spieler unten)
        let (opp_turn, opp_name) = name_block(&mut seat_root, (0.0, 0.0), (14.0, 10.0), GrowDirection::END);
        let (me_turn, me_name) = name_block(&mut seat_root, (0.0, 1.0), (14.0, -12.0), GrowDirection::BEGIN);


        // Medaillons mit Leben – je unter einer Maske, die schwebende Karten ausspart
        // (Karten liegen so sichtbar über den Medaillons)
        let opp_mask = CardCutout::new_alloc();
        seat_root.add_child(&opp_mask);
        let me_mask = CardCutout::new_alloc();
        seat_root.add_child(&me_mask);
        let (opp_row, opp_badge) =
            seat_row(&mut opp_mask.clone().upcast(), OPPONENT_ACCENT, (0.0, 0.0), (8.0, 8.0), GrowDirection::END);
        let (me_row, me_badge) =
            seat_row(&mut me_mask.clone().upcast(), PLAYER_ACCENT, (0.0, 1.0), (8.0, -8.0), GrowDirection::BEGIN);
        // Manapool des Spielers: runde Symbole rechts neben dem Medaillon (erbt die Maske)
        let mut mana = ManaPoolView::new_alloc();
        mana.set_use_parent_material(true);
        let cb = Callable::from_object_method(&self.to_gd(), "on_pool_clicked");
        mana.connect_flags("mana_clicked", &cb, ConnectFlags::DEFERRED);
        me_mask.clone().add_child(&mana);
        self.player_mana = Some(mana);

        // Knöpfe als animierte 3D-Knöpfe (orange), an die Bildschirmstelle geheftet
        let mut button = Button3D::create(ButtonStyle::Orange, &tr!("Passen  »", "Pass  »"), 1.0);
        button.set_name("PhaseButton");
        button.bind_mut().pin_to_screen(
            Vector2::new(1.0, 1.0),
            Vector2::new(-BUTTON_MARGIN - MAIN_FACE_PX * 0.5, main_y),
            Button3D::image_width_for_face(MAIN_FACE_PX, WIDE_BODY),
        );
        let cb = Callable::from_object_method(&self.to_gd(), "on_next_pressed");
        button.connect("pressed", &cb);
        self.base_mut().add_child(&button);
        let mut end_turn = Button3D::create_shaped(ButtonStyle::Orange, "»»", 1.0, COMPACT_BODY);
        end_turn.set_name("EndTurnButton");
        end_turn.bind_mut().pin_to_screen(
            Vector2::new(1.0, 1.0),
            Vector2::new(-BUTTON_MARGIN - end_w * 0.5, end_y),
            Button3D::image_width_for_face(end_w, COMPACT_BODY),
        );
        let cb = Callable::from_object_method(&self.to_gd(), "on_end_turn_pressed");
        end_turn.connect("pressed", &cb);
        self.base_mut().add_child(&end_turn);

        self.choice = Some(ChoicePanel::new(&mut root));
        // Medaillon-Klicks (Spieler als Ziel) an den ForgeTable melden
        for (seat, badge) in [(0i64, &me_badge), (1, &opp_badge)] {
            let cb = Callable::from_object_method(&self.to_gd(), "on_badge_clicked").bind(&[seat.to_variant()]);
            badge.clone().connect("clicked", &cb);
        }
        // Klick auf ein Ziel auf dem Tisch = Klick auf die Option im Auswahlfenster
        // (zurückgestellt: der ForgeTable ist beim Senden noch gebunden)
        if let Some(mut ft) = self.forge_table.clone() {
            let cb = Callable::from_object_method(&self.to_gd(), "on_choice_option");
            ft.connect_flags("choice_option_clicked", &cb, ConnectFlags::DEFERRED);
        }
        self.base_mut().add_child(&seat_layer);
        self.base_mut().add_child(&root);
        self.root = Some(root.clone());
        self.timeline = Some(timeline);
        self.opponent_name = Some(opp_name);
        self.player_name = Some(me_name);
        self.opponent_turn = Some(opp_turn);
        self.player_turn = Some(me_turn);
        self.opponent_badge = Some(opp_badge);
        self.player_badge = Some(me_badge);
        self.opponent_row = Some(opp_row);
        self.player_row = Some(me_row);
        self.opponent_mask = Some(opp_mask);
        self.player_mask = Some(me_mask);
        self.game_over_label = Some(hint);
        self.button = Some(button);
        self.end_turn_button = Some(end_turn);
        self.button_hint = Some(button_hint);
    }

    /// Zweiter Knopf über dem Phasen-Knopf: entsteht (wächst aus der Mitte), sobald der
    /// [ForgeTable] einen anbietet, wechselt Text und Farbe an Ort und Stelle und verschwindet
    /// animiert, wenn keiner mehr gebraucht wird.
    fn sync_secondary(&mut self, wanted: Option<&(GString, ButtonStyle)>) {
        match wanted {
            Some((text, style)) => {
                if let Some(b) = self.secondary_button.as_mut() {
                    let mut b = b.bind_mut();
                    b.set_style(*style);
                    b.set_label_text(text.clone());
                    return;
                }
                let mut b = Button3D::create(*style, &text.to_string(), 1.0);
                b.set_name("SecondaryButton");
                b.bind_mut().pin_to_screen(
                    Vector2::new(1.0, 1.0),
                    Vector2::new(-BUTTON_MARGIN - MAIN_FACE_PX * 0.5, self.secondary_y),
                    Button3D::image_width_for_face(MAIN_FACE_PX, WIDE_BODY),
                );
                let cb = Callable::from_object_method(&self.to_gd(), "on_secondary_pressed");
                b.connect("pressed", &cb);
                self.base_mut().add_child(&b);
                self.secondary_button = Some(b);
            }
            None => {
                if let Some(mut b) = self.secondary_button.take() {
                    b.bind_mut().dismiss();
                }
            }
        }
    }

    /// Zeitstrahl und Hinweis über dem Knopf-Stapel weich nach oben rücken, solange der zweite
    /// Knopf sichtbar ist (sonst zurück).
    fn lift_stack(&mut self, dt: f32) {
        let shown = self.secondary_button.as_ref().is_some_and(|b| b.is_visible());
        let target = if shown { self.secondary_lift } else { 0.0 };
        if (target - self.stack_lift).abs() < 0.01 {
            if self.stack_lift == target {
                return;
            }
            self.stack_lift = target;
        } else {
            self.stack_lift += (target - self.stack_lift) * (1.0 - (-STACK_LIFT_SPEED * dt).exp());
        }
        let y = self.timeline_y - self.stack_lift;
        if let Some(t) = self.timeline.as_mut() {
            place(t, (1.0, 1.0), (-BUTTON_MARGIN, y), GrowDirection::BEGIN, GrowDirection::BEGIN);
        }
        if let Some(h) = self.button_hint.as_mut() {
            place(h, (1.0, 1.0), (-BUTTON_MARGIN, y - 66.0), GrowDirection::BEGIN, GrowDirection::BEGIN);
        }
    }

    /// Setzt die Medaillon-Zeilen an ihre 3D-Anker (Medaillon mittig über/unter dem Ankerpunkt)
    /// und spart die gehoverte bzw. gezogene Karte aus der Medaillon-Maske aus.
    fn place_seat_rows(&mut self) {
        let Some(viewport) = self.base().get_viewport() else { return };
        let Some(cam) = viewport.get_camera_3d() else { return };
        let card = self.base().get_parent().and_then(|p| p.try_cast::<TableController>().ok()).and_then(|t| {
            let t = t.bind();
            t.get_dragged_card().or_else(|| t.get_hovered_card())
        });
        let quad = card.map(|c| c.bind().screen_quad()).unwrap_or_default();
        for mask in [self.opponent_mask.as_mut(), self.player_mask.as_mut()].into_iter().flatten() {
            mask.bind_mut().set_cutout(quad.clone());
        }
        for (anchor, row, is_player) in [
            (self.player_hero_anchor.clone(), self.player_row.clone(), true),
            (self.opponent_hero_anchor.clone(), self.opponent_row.clone(), false),
        ] {
            let (Some(anchor), Some(mut row)) = (anchor, row) else { continue };
            let world = anchor.get_global_position();
            if cam.is_position_behind(world) {
                continue;
            }
            let p = cam.unproject_position(world);
            // Zeile auf Mindestgröße bringen; erstes Kind ist das Medaillon
            let size = row.get_combined_minimum_size();
            row.set_size(size);
            let badge_w = row.get_child(0).map(|b| b.cast::<Control>().get_combined_minimum_size().x).unwrap_or(0.0);
            let top = if is_player { p.y - size.y } else { p.y };
            row.set_position(Vector2::new(p.x - badge_w * 0.5, top));
            // Ohne Mana-Anker: Manapool rechts neben dem Spieler-Medaillon, auf Höhe der Rautenmitte
            if is_player && self.player_mana_anchor.is_none() {
                if let Some(mana) = self.player_mana.as_mut() {
                    let min = mana.get_combined_minimum_size();
                    mana.set_size(min);
                    mana.set_position(Vector2::new(p.x + badge_w * 0.5 + 10.0, top + size.y * 0.42 - min.y * 0.5));
                }
            }
        }
        // Mana-Anker (unter der Ressourcen-Reihe): Symbole rechtsbündig links vom Anker – mehr
        // Manaarten wachsen nach links, weg vom Medaillon und den Giftmarken daneben
        if let (Some(anchor), Some(mana)) = (self.player_mana_anchor.clone(), self.player_mana.as_mut()) {
            let world = anchor.get_global_position();
            if !cam.is_position_behind(world) {
                let p = cam.unproject_position(world);
                let min = mana.get_combined_minimum_size();
                mana.set_size(min);
                mana.set_position(Vector2::new(p.x - min.x, p.y));
            }
        }
    }

    fn refresh(&mut self) {
        self.place_seat_rows();
        let Some(ft) = self.forge_table.clone() else { return };
        let view = ft.bind().hud_view();
        let active = view.active_seat;

        if let Some(t) = self.timeline.as_mut() {
            t.bind_mut().set_state(view.phase_titles.clone(), view.phase_index);
        }

        if let Some(m) = self.player_mana.as_mut() {
            m.bind_mut().set_amounts(view.player_pool.clone());
            m.bind_mut().set_clickable(&view.pool_clickable);
        }
        for (seat, badge) in [(1, self.opponent_badge.as_mut()), (0, self.player_badge.as_mut())] {
            if let Some(b) = badge {
                let mut b = b.bind_mut();
                b.set_state(view.names[seat].clone(), view.lives[seat], seat as i64 == active);
                if b.is_targetable() != view.targetable_seats[seat] {
                    b.set_targetable(view.targetable_seats[seat]);
                }
                b.set_target_selected(view.selected_seats[seat]);
            }
        }
        // Linker Bildrand: Name (golden = am Zug) und Rundenzahl neben der Sanduhr
        let turn = match view.day {
            Some(true) => format!("{} ☀ {}", view.turn, tr!("Tag", "Day")),
            Some(false) => format!("{} ☾ {}", view.turn, tr!("Nacht", "Night")),
            None => view.turn.to_string(),
        };
        for (seat, name, turn_label) in [
            (1usize, self.opponent_name.as_mut(), self.opponent_turn.as_mut()),
            (0, self.player_name.as_mut(), self.player_turn.as_mut()),
        ] {
            if let Some(l) = name {
                let text = if view.counters[seat].is_empty() {
                    view.names[seat].to_string()
                } else {
                    format!("{}\n{}", view.names[seat], view.counters[seat])
                };
                l.set_text(&text);
                // nur bei Wechsel setzen: jedes Setzen lässt Godot den Text neu setzen und auslegen
                let color = if seat as i64 == active { GOLD } else { PARCHMENT };
                if l.get_theme_color("font_color") != color {
                    l.add_theme_color_override("font_color", color);
                }
            }
            if let Some(l) = turn_label {
                l.set_text(&turn);
            }
        }

        // Keine Hinweistexte während des Spiels (Phase steht im Zeitstrahl) – nur das Spielende
        // wird in der Bildmitte angezeigt.
        let winner = view.winner.clone();
        if let Some(l) = self.game_over_label.as_mut() {
            let text = if winner.is_empty() {
                String::new()
            } else {
                trf!("Spielende\n{winner} gewinnt!", "Game over\n{winner} wins!")
            };
            l.set_text(&text);
        }
        // ausgeblendet nur, solange eine Bühne den Tisch abdeckt (sie hat eigene Knöpfe); bei
        // Bühnen am Tisch (z. B. X wählen) bleibt der Phasen-Knopf – beim Wirken „Abbrechen“
        self.sync_secondary(view.secondary_button.as_ref());
        for b in [self.button.as_mut(), self.end_turn_button.as_mut(), self.secondary_button.as_mut()].into_iter().flatten() {
            b.set_visible(!view.stage_covers);
        }
        // 3D-Bühne offen (z. B. Starthand beim Mulligan, Abwerfen): Medaillons, Namen und Manapool
        // ausgrauen – der Blick gehört der Bühne
        let tint = if view.stage_open { STAGE_TINT } else { Color::WHITE };
        for m in [self.opponent_mask.as_mut(), self.player_mask.as_mut()].into_iter().flatten() {
            m.set_modulate(tint);
            m.set_visible(!view.stage_covers);
        }
        if let Some(tl) = self.timeline.as_mut() {
            tl.set_visible(!view.stage_covers);
        }
        for l in [self.opponent_name.as_mut(), self.player_name.as_mut(), self.opponent_turn.as_mut(), self.player_turn.as_mut()]
            .into_iter()
            .flatten()
        {
            if let Some(mut parent) = l.get_parent().and_then(|p| p.try_cast::<Control>().ok()) {
                parent.set_modulate(tint);
                parent.set_visible(!view.stage_covers);
            }
        }
        if let Some(b) = self.button.as_mut() {
            let mut b = b.bind_mut();
            b.set_enabled(view.button_enabled);
            b.set_label_text(view.button_text.clone());
        }
        if let Some(b) = self.end_turn_button.as_mut() {
            b.bind_mut().set_enabled(view.end_turn_enabled);
        }
        if self.shown_hint != view.button_hint {
            self.shown_hint = view.button_hint.clone();
            if let Some(l) = self.button_hint.as_mut() {
                super::mana_symbols::fill_rich(l, &view.button_hint, 17);
            }
        }

        // Auswahlfenster (Mulligan, Optionen, Ja/Nein, Zahlen, Ziele …); die aktuelle Auswahl
        // geht an den ForgeTable (Leuchten und Pfeile auf dem Tisch)
        let hud = self.to_gd().upcast::<Object>();
        let mut selection = PackedInt64Array::new();
        if let Some(panel) = self.choice.as_mut() {
            panel.show(view.choice.as_ref(), &hud);
            for i in panel.selected() {
                selection.push(*i as i64);
            }
        }
        if let Some(mut ft) = self.forge_table.clone() {
            ft.bind_mut().set_choice_selection(selection);
        }
    }
}

/// Alles, was das HUD anzeigt (geliefert vom [ForgeTable]). Sitzplatz 0 = Spieler (unten),
/// 1 = Gegner (oben).
pub struct HudView {
    pub phase_titles: PackedStringArray,
    /// Index in `phase_titles` (−1 = keine Phase markiert).
    pub phase_index: i64,
    /// Sitzplatz am Zug (−1 = keiner).
    pub active_seat: i64,
    pub names: [GString; 2],
    pub lives: [i64; 2],
    /// Kennzeichen und Marken je Sitzplatz als Text (Monarch, Initiative, Gift …; leer = keine),
    /// unter dem Namen.
    pub counters: [String; 2],
    /// Tag (true) oder Nacht (false), CR 726; None = noch keins von beiden.
    pub day: Option<bool>,
    pub turn: i64,
    /// Name des Siegers; leer = Partie läuft.
    pub winner: String,
    pub button_enabled: bool,
    pub button_text: GString,
    /// Hinweis der Engine unter dem Phasen-Knopf (z. B. ungültiger Angriff; leer = keiner).
    pub button_hint: String,
    /// Zweiter Knopf über dem Phasen-Knopf (Text, Farbstil), z. B. „Kein Angriff“ (blau) oder
    /// „Angriff abbrechen“ (grau); None = keiner.
    pub secondary_button: Option<(GString, ButtonStyle)>,
    /// Knopf „»»“ (Sofort beenden) anklickbar: Partie läuft, eigener Zug, nicht schon aktiv.
    pub end_turn_enabled: bool,
    /// Eine 3D-Bühne (Startspieler, Mulligan) ist offen: Medaillons und Namen ausgrauen.
    pub stage_open: bool,
    /// Die offene Bühne deckt den Tisch ab (abgedunkelt): Medaillons, Namen, Phasen-Zeitstrahl
    /// ganz ausblenden, damit nichts über Titel, Karten und Knöpfen der Bühne liegt.
    pub stage_covers: bool,
    /// Manapool des Spielers (je [ManaType] eine Menge; leer = keine Anzeige).
    pub player_pool: PackedInt64Array,
    /// Je Manaart: beim Bezahlen anklickbar (schwebendes Mana einsetzen).
    pub pool_clickable: Vec<bool>,
    pub choice: Option<HudChoice>,
    /// Ist Sitzplatz 0 / 1 gerade ein gültiges Ziel (Medaillon anklickbar)?
    pub targetable_seats: [bool; 2],
    /// Ist Sitzplatz 0 / 1 als Ziel gewählt?
    pub selected_seats: [bool; 2],
}

pub(super) fn styled_button(font_size: i32) -> Gd<Button> {
    let mut button = Button::new_alloc();
    button.set_focus_mode(godot::classes::control::FocusMode::NONE);
    button.set_custom_minimum_size(Vector2::new(0.0, 38.0));
    button.add_theme_font_size_override("font_size", font_size);
    button.add_theme_color_override("font_color", PARCHMENT);
    button.add_theme_color_override("font_hover_color", GOLD);
    button.add_theme_color_override("font_disabled_color", DIM);
    for (state, bg) in [
        ("normal", Color::from_rgba(0.14, 0.17, 0.24, 0.95)),
        ("hover", Color::from_rgba(0.2, 0.25, 0.36, 0.95)),
        ("pressed", Color::from_rgba(0.09, 0.11, 0.16, 0.95)),
        ("disabled", Color::from_rgba(0.08, 0.09, 0.12, 0.7)),
    ] {
        button.add_theme_stylebox_override(state, &style(bg, 6.0));
    }
    button
}

/// Rundenzahl mit Sanduhr und darunter der Spielername, am linken Bildrand verankert
/// (`anchor`/`offset` wie bei [`place`]). Gibt Rundenzahl- und Namens-Label zurück.
fn name_block(root: &mut Gd<Control>, anchor: (f32, f32), offset: (f32, f32), grow_v: GrowDirection) -> (Gd<Label>, Gd<Label>) {
    let mut col = VBoxContainer::new_alloc();
    col.add_theme_constant_override("separation", 0);
    col.set_mouse_filter(MouseFilter::IGNORE);
    let mut turn_row = godot::classes::HBoxContainer::new_alloc();
    turn_row.add_theme_constant_override("separation", 6);
    turn_row.set_mouse_filter(MouseFilter::IGNORE);
    let mut glass = HourglassIcon::new_alloc();
    glass.set_v_size_flags(godot::classes::control::SizeFlags::SHRINK_CENTER);
    turn_row.add_child(&glass);
    let turn = label(16, PARCHMENT);
    turn_row.add_child(&turn);
    col.add_child(&turn_row);
    let name = label(18, PARCHMENT);
    col.add_child(&name);
    place(&mut col, anchor, offset, GrowDirection::END, grow_v);
    root.add_child(&col);
    (turn, name)
}

/// Medaillon einer Seite, an einer Bildschirmecke verankert (Rückfall, falls kein
/// 3D-Anker gesetzt ist). Gibt Zeile und Medaillon zurück.
fn seat_row(
    root: &mut Gd<Control>,
    accent: Color,
    anchor: (f32, f32),
    offset: (f32, f32),
    grow_v: GrowDirection,
) -> (Gd<Control>, Gd<SeatBadge>) {
    let mut row = godot::classes::HBoxContainer::new_alloc();
    row.add_theme_constant_override("separation", 4);
    row.set_mouse_filter(MouseFilter::IGNORE);
    // Material (Aussparung/Ausgrauen) der umgebenden CardCutout-Maske erben
    row.set_use_parent_material(true);
    let mut badge = SeatBadge::new_alloc();
    badge.set("accent", &accent.to_variant());
    badge.set_use_parent_material(true);
    row.add_child(&badge);
    place(&mut row, anchor, offset, GrowDirection::END, grow_v);
    root.add_child(&row);
    (row.upcast(), badge)
}

pub(super) fn label(size: i32, color: Color) -> Gd<Label> {
    let mut l = Label::new_alloc();
    l.add_theme_font_size_override("font_size", size);
    l.add_theme_color_override("font_color", color);
    l.add_theme_color_override("font_outline_color", Color::from_rgba(0.0, 0.0, 0.0, 0.8));
    l.add_theme_constant_override("outline_size", 4);
    l.set_mouse_filter(MouseFilter::IGNORE);
    l
}

pub(super) fn style(bg: Color, radius: f32) -> Gd<StyleBoxFlat> {
    let mut s = StyleBoxFlat::new_gd();
    s.set_bg_color(bg);
    s.set_border_color(Color::from_rgba(0.78, 0.6, 0.3, 0.9));
    s.set_border_width_all(2);
    s.set_corner_radius_all(radius as i32);
    s.set_content_margin_all(10.0);
    s
}

pub(super) fn panel() -> Gd<PanelContainer> {
    let mut p = PanelContainer::new_alloc();
    p.add_theme_stylebox_override("panel", &style(Color::from_rgba(0.05, 0.06, 0.08, 0.82), 8.0));
    p.set_mouse_filter(MouseFilter::IGNORE);
    p
}

/// Verankert ein Control an einer Bildschirmecke (anchor 0/1) mit Abstand `offset`.
pub(super) fn place<T: Inherits<Control>>(c: &mut Gd<T>, anchor: (f32, f32), offset: (f32, f32), grow_h: GrowDirection, grow_v: GrowDirection) {
    for (side, a, o) in [
        (Side::LEFT, anchor.0, offset.0),
        (Side::RIGHT, anchor.0, offset.0),
        (Side::TOP, anchor.1, offset.1),
        (Side::BOTTOM, anchor.1, offset.1),
    ] {
        let mut c = c.clone().upcast::<Control>();
        c.set_anchor(side, a);
        c.set_offset(side, o);
    }
    let mut c = c.clone().upcast::<Control>();
    c.set_h_grow_direction(grow_h);
    c.set_v_grow_direction(grow_v);
}
