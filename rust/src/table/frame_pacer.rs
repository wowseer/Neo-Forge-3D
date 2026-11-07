//! FramePacer: begrenzt die Bildrate, damit der Tisch nicht mehr rechnet, als man sieht.
//!
//! Ein Kartenspiel braucht keine 144 Bilder pro Sekunde, und ein ruhender Tisch keine 60: Solange
//! sich etwas tut (Eingaben, Nachrichten der Engine und kurz danach, solange Animationen laufen),
//! zeichnet der Tisch mit `active_fps`, in Ruhe mit `idle_fps`, minimiert mit `minimized_fps`.
//! Aktivität meldet jede Eingabe von selbst; andere Quellen (z. B. die Brücke bei neuen
//! Nachrichten) rufen [`wake`].

use std::cell::Cell;

use godot::classes::display_server::{VSyncMode, WindowMode};
use godot::classes::notify::NodeNotification;
use godot::classes::{DisplayServer, Engine, INode, InputEvent, Node, Time};
use godot::prelude::*;

thread_local! {
    /// Zeitpunkt der letzten Aktivität (Millisekunden seit Programmstart).
    static LAST_WAKE: Cell<u64> = const { Cell::new(0) };
}

/// Meldet Aktivität (z. B. eine neue Nachricht der Engine): Der Tisch zeichnet wieder mit voller
/// Bildrate, bis [member FramePacer.hold_seconds] lang nichts mehr passiert.
pub fn wake() {
    LAST_WAKE.with(|t| t.set(Time::singleton().get_ticks_msec()));
}

/// Begrenzt die Bildrate (`Engine.max_fps`) je nach Lage: volle Rate, solange sich etwas tut
/// (Eingaben, Nachrichten der Engine), gedrosselt in Ruhe und stark gedrosselt im minimierten
/// Fenster. Spart Strom und Abwärme, ohne dass Bewegungen ruckeln: Jede Eingabe schaltet sofort
/// auf die volle Rate zurück.
#[derive(GodotClass)]
#[class(base=Node)]
pub struct FramePacer {
    /// Bildrate, solange sich etwas tut (0 = unbegrenzt). Liegt die Bildwiederholrate des
    /// Monitors (mit V-Sync) nicht höher, begrenzt V-Sync allein.
    #[export]
    active_fps: i32,
    /// Bildrate in Ruhe: keine Eingabe und keine Nachricht der Engine seit [member hold_seconds].
    #[export]
    idle_fps: i32,
    /// Bildrate, solange das Fenster minimiert ist.
    #[export]
    minimized_fps: i32,
    /// Sekunden nach der letzten Aktivität, bis auf [member idle_fps] gedrosselt wird (lang genug
    /// für die Animationen, die eine Aktivität auslöst).
    #[export]
    hold_seconds: f64,
    /// Zuletzt gesetzte Begrenzung (−1 = noch keine).
    applied: i32,
    base: Base<Node>,
}

#[godot_api]
impl INode for FramePacer {
    fn init(base: Base<Node>) -> Self {
        Self { active_fps: 60, idle_fps: 30, minimized_fps: 10, hold_seconds: 2.0, applied: -1, base }
    }

    fn ready(&mut self) {
        wake();
    }

    fn input(&mut self, _event: Gd<InputEvent>) {
        // nur beobachten, nicht verbrauchen
        wake();
    }

    fn on_notification(&mut self, what: NodeNotification) {
        if matches!(what, NodeNotification::APPLICATION_FOCUS_IN | NodeNotification::WM_WINDOW_FOCUS_IN) {
            wake();
        }
    }

    fn process(&mut self, _delta: f64) {
        let ds = DisplayServer::singleton();
        let fps = if ds.window_get_mode() == WindowMode::MINIMIZED {
            self.minimized_fps
        } else if (Time::singleton().get_ticks_msec().saturating_sub(LAST_WAKE.with(Cell::get)) as f64) > self.hold_seconds * 1000.0 {
            self.idle_fps
        } else {
            // Mit V-Sync auf einem Monitor bis zu dieser Rate: V-Sync allein begrenzt (beide
            // zusammen bei gleicher Rate lassen Bilder ausfallen)
            let refresh = ds.screen_get_refresh_rate();
            let vsync = ds.window_get_vsync_mode() != VSyncMode::DISABLED;
            if vsync && refresh > 0.0 && refresh < self.active_fps as f32 + 2.0 { 0 } else { self.active_fps }
        };
        if fps != self.applied {
            self.applied = fps;
            Engine::singleton().set_max_fps(fps.max(0));
        }
    }
}
