//! `OptionsMenu`: Zahnrad oben rechts im HUD; ein Klick öffnet wie Arenas Spielmenü ein Menü in
//! der Bildmitte über dem abgedunkelten Tisch, die Knöpfe untereinander: **Weiterspielen**,
//! **Vollbild** an/aus und **Aufgeben**. Aufgeben öffnet ein eigenes Fenster „Willst du wirklich
//! aufgeben?“; bestätigt meldet es [signal OptionsMenu.concede_requested], der
//! [`ForgeTable`](crate::forge::table::ForgeTable) schickt es an die Brücke und die Partie endet
//! wie gewohnt mit dem Ergebnis. Klick auf die Abdunkelung = Weiterspielen.

use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter};
use godot::classes::display_server::WindowMode;
use godot::classes::{
    Button, CenterContainer, ColorRect, Control, DisplayServer, HBoxContainer, IControl, InputEvent, InputEventMouseButton,
    Label, PanelContainer, VBoxContainer,
};
use godot::global::{HorizontalAlignment, MouseButton};
use godot::prelude::*;

use super::hud::{panel, place, styled_button};

/// Größe des Zahnrads und Abstand zum Bildrand (Pixel).
const GEAR_SIZE: f32 = 46.0;
const GEAR_MARGIN: f32 = 14.0;
/// Breite der Knöpfe im Menü und im Rückfrage-Fenster (Pixel).
const MENU_WIDTH: f32 = 360.0;
const CONFIRM_WIDTH: f32 = 440.0;
/// Abdunkelung des Tisches hinter dem Menü.
const DIM: Color = Color::from_rgba(0.0, 0.0, 0.0, 0.62);

const GOLD: Color = Color::from_rgb(0.95, 0.78, 0.4);
const PARCHMENT: Color = Color::from_rgb(0.93, 0.87, 0.74);
const RED: Color = Color::from_rgb(1.0, 0.45, 0.35);

/// Zahnrad-Symbol (gezeichnet): Kranz mit acht Zähnen und Loch in der Mitte.
#[derive(GodotClass)]
#[class(base=Control)]
pub struct GearIcon {
    hovered: bool,
    open: bool,
    base: Base<Control>,
}

#[godot_api]
impl IControl for GearIcon {
    fn init(base: Base<Control>) -> Self {
        Self { hovered: false, open: false, base }
    }

    fn draw(&mut self) {
        use std::f32::consts::TAU;
        let size = self.base().get_size();
        let c = size * 0.5;
        let r = size.x.min(size.y) * 0.5;
        let ink = if self.hovered || self.open { GOLD } else { PARCHMENT };
        let mut b = self.base_mut();
        // dunkle runde Unterlage, damit das Zahnrad auf jedem Spielfeld zu sehen ist
        b.draw_circle(c, r, Color::from_rgba(0.05, 0.06, 0.08, 0.78));
        b.draw_arc_ex(c, r - 1.0, 0.0, TAU, 48, Color::from_rgba(0.78, 0.6, 0.3, 0.9)).width(2.0).antialiased(true).done();
        let outer = r * 0.72;
        let body = r * 0.56;
        // acht Zähne als Trapeze um den Kranz
        for i in 0..8 {
            let a = i as f32 * TAU / 8.0;
            let (half_in, half_out) = (0.2f32, 0.13f32);
            let p = |ang: f32, rad: f32| c + Vector2::new(ang.cos(), ang.sin()) * rad;
            let pts = PackedVector2Array::from(&[
                p(a - half_in, body * 0.95),
                p(a - half_out, outer),
                p(a + half_out, outer),
                p(a + half_in, body * 0.95),
            ]);
            b.draw_colored_polygon(&pts, ink);
        }
        b.draw_circle(c, body, ink);
        b.draw_circle(c, r * 0.24, Color::from_rgba(0.05, 0.06, 0.08, 1.0));
    }
}

impl GearIcon {
    fn set_state(&mut self, hovered: bool, open: bool) {
        self.hovered = hovered;
        self.open = open;
        self.base_mut().queue_redraw();
    }
}

/// Zahnrad oben rechts mit dem Spielmenü (siehe Modul-Doku).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct OptionsMenu {
    gear: Option<Gd<GearIcon>>,
    /// Abdunkelung + Menü in der Mitte.
    overlay: Option<Gd<Control>>,
    menu: Option<Gd<CenterContainer>>,
    confirm: Option<Gd<CenterContainer>>,
    fullscreen: Option<Gd<Button>>,
    hovered: bool,
    base: Base<Control>,
}

#[godot_api]
impl IControl for OptionsMenu {
    fn init(base: Base<Control>) -> Self {
        Self { gear: None, overlay: None, menu: None, confirm: None, fullscreen: None, hovered: false, base }
    }

    fn ready(&mut self) {
        self.build();
    }
}

#[godot_api]
impl OptionsMenu {
    /// Der Spieler will aufgeben (im Rückfrage-Fenster bestätigt).
    #[signal]
    fn concede_requested();

    #[func]
    fn on_gear_pressed(&mut self) {
        let open = !self.is_open();
        self.set_open(open);
    }

    #[func]
    fn on_gear_hover(&mut self, hovered: bool) {
        self.hovered = hovered;
        self.refresh_gear();
    }

    #[func]
    fn on_fullscreen(&mut self) {
        let mut ds = DisplayServer::singleton();
        let full = matches!(ds.window_get_mode(), WindowMode::FULLSCREEN | WindowMode::EXCLUSIVE_FULLSCREEN);
        ds.window_set_mode(if full { WindowMode::MAXIMIZED } else { WindowMode::FULLSCREEN });
        self.refresh_labels();
    }

    /// „Aufgeben“ im Menü: Rückfrage-Fenster statt des Menüs.
    #[func]
    fn on_concede(&mut self) {
        self.show_page(false);
    }

    /// Rückfrage bestätigt: aufgeben.
    #[func]
    fn on_concede_confirmed(&mut self) {
        self.set_open(false);
        self.signals().concede_requested().emit();
    }

    /// Rückfrage abgebrochen: zurück ins Menü.
    #[func]
    fn on_concede_cancelled(&mut self) {
        self.show_page(true);
    }

    #[func]
    fn on_resume(&mut self) {
        self.set_open(false);
    }

    /// Klick auf die Abdunkelung neben dem Menü: weiterspielen.
    #[func]
    fn on_dim_input(&mut self, event: Gd<InputEvent>) {
        if let Ok(mb) = event.try_cast::<InputEventMouseButton>() {
            if mb.is_pressed() && mb.get_button_index() == MouseButton::LEFT {
                self.set_open(false);
            }
        }
    }
}

impl OptionsMenu {
    /// Ist das Menü offen?
    pub fn is_open(&self) -> bool {
        self.overlay.as_ref().is_some_and(|o| o.is_visible())
    }

    /// Menü öffnen (immer mit der Hauptseite) bzw. schließen.
    pub fn set_open(&mut self, open: bool) {
        if let Some(o) = self.overlay.as_mut() {
            o.set_visible(open);
        }
        self.show_page(true);
        self.refresh_labels();
        self.refresh_gear();
    }

    /// Hauptmenü (`true`) oder Rückfrage-Fenster zeigen.
    fn show_page(&mut self, main: bool) {
        if let Some(m) = self.menu.as_mut() {
            m.set_visible(main);
        }
        if let Some(c) = self.confirm.as_mut() {
            c.set_visible(!main);
        }
    }

    fn refresh_gear(&mut self) {
        let (hovered, open) = (self.hovered, self.is_open());
        if let Some(g) = self.gear.as_mut() {
            g.bind_mut().set_state(hovered, open);
        }
    }

    fn refresh_labels(&mut self) {
        let full = matches!(DisplayServer::singleton().window_get_mode(), WindowMode::FULLSCREEN | WindowMode::EXCLUSIVE_FULLSCREEN);
        if let Some(b) = self.fullscreen.as_mut() {
            b.set_text(if full { crate::tr!("Vollbild beenden", "Exit full screen") } else { crate::tr!("Vollbild", "Full screen") });
        }
    }

    fn build(&mut self) {
        let mut base = self.base_mut().clone();
        base.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        base.set_mouse_filter(MouseFilter::IGNORE);
        let me = self.to_gd();

        // Abdunkelung über dem ganzen Tisch (fängt Klicks ab), darüber das Menü in der Mitte
        let mut overlay = Control::new_alloc();
        overlay.set_name("Overlay");
        overlay.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        overlay.set_mouse_filter(MouseFilter::IGNORE);
        let mut dim = ColorRect::new_alloc();
        dim.set_color(DIM);
        dim.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        dim.set_mouse_filter(MouseFilter::STOP);
        dim.connect("gui_input", &Callable::from_object_method(&me, "on_dim_input"));
        overlay.add_child(&dim);

        let button = |text: &str, method: &str, width: f32, color: Color| {
            let mut b = styled_button(24);
            b.set_text(text);
            b.set_custom_minimum_size(Vector2::new(width, 54.0));
            b.add_theme_color_override("font_color", color);
            b.connect("pressed", &Callable::from_object_method(&me, method));
            b
        };
        let title = |text: &str| {
            let mut l = Label::new_alloc();
            l.set_text(text);
            l.set_horizontal_alignment(HorizontalAlignment::CENTER);
            l.add_theme_font_size_override("font_size", 30);
            l.add_theme_color_override("font_color", GOLD);
            l
        };
        let centered = |content: &Gd<PanelContainer>| {
            let mut c = CenterContainer::new_alloc();
            c.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
            c.set_mouse_filter(MouseFilter::IGNORE);
            c.add_child(content);
            c
        };

        // Hauptseite: Knöpfe untereinander
        let mut menu_panel = panel();
        menu_panel.set_mouse_filter(MouseFilter::STOP);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 12);
        col.add_child(&title(crate::tr!("Optionen", "Options")));
        col.add_child(&button(crate::tr!("Weiterspielen", "Resume"), "on_resume", MENU_WIDTH, PARCHMENT));
        let fullscreen = button(crate::tr!("Vollbild", "Full screen"), "on_fullscreen", MENU_WIDTH, PARCHMENT);
        col.add_child(&fullscreen);
        col.add_child(&button(crate::tr!("Aufgeben", "Concede"), "on_concede", MENU_WIDTH, RED));
        menu_panel.add_child(&col);
        let menu = centered(&menu_panel);
        overlay.add_child(&menu);

        // Rückfrage-Fenster
        let mut confirm_panel = panel();
        confirm_panel.set_mouse_filter(MouseFilter::STOP);
        let mut ccol = VBoxContainer::new_alloc();
        ccol.add_theme_constant_override("separation", 18);
        ccol.add_child(&title(crate::tr!("Aufgeben", "Concede")));
        let mut question = Label::new_alloc();
        question.set_text(crate::tr!("Willst du wirklich aufgeben?", "Do you really want to concede?"));
        question.set_horizontal_alignment(HorizontalAlignment::CENTER);
        question.set_custom_minimum_size(Vector2::new(CONFIRM_WIDTH, 0.0));
        question.add_theme_font_size_override("font_size", 24);
        question.add_theme_color_override("font_color", PARCHMENT);
        ccol.add_child(&question);
        let mut row = HBoxContainer::new_alloc();
        row.add_theme_constant_override("separation", 16);
        row.add_child(&button(crate::tr!("Abbrechen", "Cancel"), "on_concede_cancelled", (CONFIRM_WIDTH - 16.0) * 0.5, PARCHMENT));
        row.add_child(&button(crate::tr!("Aufgeben", "Concede"), "on_concede_confirmed", (CONFIRM_WIDTH - 16.0) * 0.5, RED));
        ccol.add_child(&row);
        confirm_panel.add_child(&ccol);
        let confirm = centered(&confirm_panel);
        confirm.clone().set_visible(false);
        overlay.add_child(&confirm);
        overlay.set_visible(false);
        base.add_child(&overlay);

        // Zahnrad oben rechts (über der Abdunkelung: schließt das Menü wieder)
        let mut gear_button = Button::new_alloc();
        gear_button.set_flat(true);
        gear_button.set_focus_mode(godot::classes::control::FocusMode::NONE);
        gear_button.set_custom_minimum_size(Vector2::splat(GEAR_SIZE));
        gear_button.set_tooltip_text(crate::tr!("Optionen", "Options"));
        place(&mut gear_button, (1.0, 0.0), (-GEAR_MARGIN, GEAR_MARGIN), GrowDirection::BEGIN, GrowDirection::END);
        let mut gear = GearIcon::new_alloc();
        gear.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        gear.set_mouse_filter(MouseFilter::IGNORE);
        gear_button.add_child(&gear);
        gear_button.connect("pressed", &Callable::from_object_method(&me, "on_gear_pressed"));
        gear_button.connect("mouse_entered", &Callable::from_object_method(&me, "on_gear_hover").bind(&[true.to_variant()]));
        gear_button.connect("mouse_exited", &Callable::from_object_method(&me, "on_gear_hover").bind(&[false.to_variant()]));
        base.add_child(&gear_button);

        self.gear = Some(gear);
        self.overlay = Some(overlay);
        self.menu = Some(menu);
        self.confirm = Some(confirm);
        self.fullscreen = Some(fullscreen);
        self.refresh_labels();
    }
}
