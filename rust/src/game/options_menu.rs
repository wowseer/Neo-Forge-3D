//! `OptionsMenu`: Zahnrad oben rechts im HUD (wie Arena) mit einem kleinen Optionen-Menü:
//! **Vollbild** an/aus, **Aufgeben** (mit Rückfrage) und **Weiterspielen**. Aufgeben meldet
//! [signal OptionsMenu.concede_requested]; der [`ForgeTable`](crate::forge::table::ForgeTable)
//! schickt es an die Brücke, die Partie endet dann wie gewohnt mit dem Ergebnis.

use godot::classes::control::{GrowDirection, LayoutPreset, MouseFilter};
use godot::classes::display_server::WindowMode;
use godot::classes::{Button, Control, DisplayServer, IControl, Label, PanelContainer, VBoxContainer};
use godot::global::HorizontalAlignment;
use godot::prelude::*;

use super::hud::{panel, place, styled_button};

/// Größe des Zahnrads und Abstand zum Bildrand (Pixel).
const GEAR_SIZE: f32 = 46.0;
const GEAR_MARGIN: f32 = 14.0;
/// Breite des Menüs (Pixel).
const MENU_WIDTH: f32 = 260.0;

const GOLD: Color = Color::from_rgb(0.95, 0.78, 0.4);
const PARCHMENT: Color = Color::from_rgb(0.93, 0.87, 0.74);

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

/// Zahnrad oben rechts mit Optionen-Menü (siehe Modul-Doku).
#[derive(GodotClass)]
#[class(base=Control)]
pub struct OptionsMenu {
    gear: Option<Gd<GearIcon>>,
    menu: Option<Gd<PanelContainer>>,
    fullscreen: Option<Gd<Button>>,
    concede: Option<Gd<Button>>,
    /// Aufgeben gedrückt: zweiter Druck bestätigt.
    confirming: bool,
    hovered: bool,
    base: Base<Control>,
}

#[godot_api]
impl IControl for OptionsMenu {
    fn init(base: Base<Control>) -> Self {
        Self { gear: None, menu: None, fullscreen: None, concede: None, confirming: false, hovered: false, base }
    }

    fn ready(&mut self) {
        self.build();
    }
}

#[godot_api]
impl OptionsMenu {
    /// Der Spieler will aufgeben (bestätigt).
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
        let mode = ds.window_get_mode();
        let full = matches!(mode, WindowMode::FULLSCREEN | WindowMode::EXCLUSIVE_FULLSCREEN);
        ds.window_set_mode(if full { WindowMode::MAXIMIZED } else { WindowMode::FULLSCREEN });
        self.refresh_labels();
    }

    #[func]
    fn on_concede(&mut self) {
        if !self.confirming {
            self.confirming = true;
            self.refresh_labels();
            return;
        }
        self.set_open(false);
        self.signals().concede_requested().emit();
    }

    #[func]
    fn on_resume(&mut self) {
        self.set_open(false);
    }
}

impl OptionsMenu {
    /// Ist das Menü offen?
    pub fn is_open(&self) -> bool {
        self.menu.as_ref().is_some_and(|m| m.is_visible())
    }

    /// Menü öffnen bzw. schließen (schließen setzt die Rückfrage zurück).
    pub fn set_open(&mut self, open: bool) {
        if let Some(m) = self.menu.as_mut() {
            m.set_visible(open);
        }
        if !open {
            self.confirming = false;
        }
        self.refresh_labels();
        self.refresh_gear();
    }

    fn refresh_gear(&mut self) {
        let (hovered, open) = (self.hovered, self.is_open());
        if let Some(g) = self.gear.as_mut() {
            g.bind_mut().set_state(hovered, open);
        }
    }

    fn refresh_labels(&mut self) {
        let mode = DisplayServer::singleton().window_get_mode();
        let full = matches!(mode, WindowMode::FULLSCREEN | WindowMode::EXCLUSIVE_FULLSCREEN);
        if let Some(b) = self.fullscreen.as_mut() {
            b.set_text(if full { tr!("Vollbild beenden", "Exit full screen") } else { tr!("Vollbild", "Full screen") });
        }
        let confirming = self.confirming;
        if let Some(b) = self.concede.as_mut() {
            b.set_text(if confirming { tr!("Wirklich aufgeben?", "Really concede?") } else { tr!("Aufgeben", "Concede") });
            b.add_theme_color_override(
                "font_color",
                if confirming { Color::from_rgb(1.0, 0.45, 0.35) } else { PARCHMENT },
            );
        }
    }

    fn build(&mut self) {
        let mut base = self.base_mut().clone();
        base.set_anchors_and_offsets_preset(LayoutPreset::FULL_RECT);
        base.set_mouse_filter(MouseFilter::IGNORE);
        let me = self.to_gd();

        // Zahnrad: unsichtbarer Knopf (Klickfläche) mit dem gezeichneten Symbol
        let mut button = Button::new_alloc();
        button.set_flat(true);
        button.set_focus_mode(godot::classes::control::FocusMode::NONE);
        button.set_custom_minimum_size(Vector2::splat(GEAR_SIZE));
        button.set_tooltip_text(tr!("Optionen", "Options"));
        place(&mut button, (1.0, 0.0), (-GEAR_MARGIN, GEAR_MARGIN), GrowDirection::BEGIN, GrowDirection::END);
        button.set_size(Vector2::splat(GEAR_SIZE));
        let mut gear = GearIcon::new_alloc();
        gear.set_anchors_preset(LayoutPreset::FULL_RECT);
        gear.set_mouse_filter(MouseFilter::IGNORE);
        button.add_child(&gear);
        button.connect("pressed", &Callable::from_object_method(&me, "on_gear_pressed"));
        button.connect("mouse_entered", &Callable::from_object_method(&me, "on_gear_hover").bind(&[true.to_variant()]));
        button.connect("mouse_exited", &Callable::from_object_method(&me, "on_gear_hover").bind(&[false.to_variant()]));
        base.add_child(&button);

        // Menü unter dem Zahnrad
        let mut menu = panel();
        menu.set_mouse_filter(MouseFilter::STOP);
        menu.set_custom_minimum_size(Vector2::new(MENU_WIDTH, 0.0));
        place(&mut menu, (1.0, 0.0), (-GEAR_MARGIN, GEAR_MARGIN + GEAR_SIZE + 8.0), GrowDirection::BEGIN, GrowDirection::END);
        let mut col = VBoxContainer::new_alloc();
        col.add_theme_constant_override("separation", 8);
        let mut title = Label::new_alloc();
        title.set_text(tr!("Optionen", "Options"));
        title.set_horizontal_alignment(HorizontalAlignment::CENTER);
        title.add_theme_font_size_override("font_size", 22);
        title.add_theme_color_override("font_color", GOLD);
        col.add_child(&title);
        let mut add = |text: &str, method: &str| {
            let mut b = styled_button(19);
            b.set_text(text);
            b.connect("pressed", &Callable::from_object_method(&me, method));
            col.add_child(&b);
            b
        };
        let fullscreen = add(tr!("Vollbild", "Full screen"), "on_fullscreen");
        let concede = add(tr!("Aufgeben", "Concede"), "on_concede");
        add(tr!("Weiterspielen", "Resume"), "on_resume");
        menu.add_child(&col);
        menu.set_visible(false);
        base.add_child(&menu);

        self.gear = Some(gear);
        self.menu = Some(menu);
        self.fullscreen = Some(fullscreen);
        self.concede = Some(concede);
        self.refresh_labels();
    }
}
