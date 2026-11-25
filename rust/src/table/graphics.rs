//! Grafikstufe des Tisches: volle Effekte auf einer eigenen Grafikkarte, sparsame auf
//! integrierten GPUs (Notebooks, Steam Deck …), wo der Tisch sonst ruckelt.
//!
//! Gemessen bei 1920×1080 auf einer Intel UHD Graphics (CML GT2), GPU-Zeit pro Bild beider
//! 3D-Ebenen: vorher 76 ms (13 fps); [`Quality::High`] (Vordergrund ohne SSAO) 70 ms;
//! [`Quality::Low`] 27 ms (35 fps). FSR statt bilinear kostete dort 5 ms mehr. Die 2D-Anzeigen
//! bleiben in voller Auflösung.
//!
//! Wahl: Programmargument `--graphics=high|low` (nach `--`), sonst Umgebungsvariable
//! `CLASH_GRAPHICS=high|low`, sonst automatisch: [`Quality::High`] nur auf einer eigenen
//! Grafikkarte (`RenderingServer.get_video_adapter_type() == DISCRETE_GPU`).

use std::sync::OnceLock;

use godot::classes::rendering_device::DeviceType;
use godot::classes::viewport::Scaling3DMode;
use godot::classes::{Environment, Os, RenderingServer, Viewport};
use godot::obj::Singleton;
use godot::prelude::*;

/// 3D-Auflösung in der sparsamen Stufe (Anteil der Fenstergröße, bilinear hochgerechnet).
const LOW_SCALE_3D: f32 = 0.75;

/// Grafikstufe des Tisches.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Quality {
    /// Alle Effekte (SSAO, Glow), volle 3D-Auflösung.
    High,
    /// Ohne SSAO und Glow, 3D-Auflösung [`LOW_SCALE_3D`] (bilinear hochgerechnet).
    Low,
}

/// Grafikstufe dieses Starts (einmal bestimmt).
pub fn quality() -> Quality {
    static QUALITY: OnceLock<Quality> = OnceLock::new();
    *QUALITY.get_or_init(|| {
        let (quality, source) = match requested() {
            Some(q) => (q, "gewählt"),
            None => (detect(), "automatisch"),
        };
        godot_print!("Grafikstufe: {quality:?} ({source})");
        quality
    })
}

/// Ausdrücklich gewählte Stufe: Argument vor Umgebungsvariable.
fn requested() -> Option<Quality> {
    let parse = |v: &str| match v.trim().to_ascii_lowercase().as_str() {
        "high" | "hoch" => Some(Quality::High),
        "low" | "niedrig" => Some(Quality::Low),
        _ => None,
    };
    let args = Os::singleton().get_cmdline_user_args();
    let from_arg = args
        .as_slice()
        .iter()
        .find_map(|a| a.to_string().strip_prefix("--graphics=").and_then(parse));
    from_arg.or_else(|| std::env::var("CLASH_GRAPHICS").ok().as_deref().and_then(parse))
}

/// Automatisch: eigene Grafikkarte = hoch, sonst (integriert, virtuell, CPU, unbekannt) sparsam.
fn detect() -> Quality {
    if RenderingServer::singleton().get_video_adapter_type() == DeviceType::DISCRETE_GPU {
        Quality::High
    } else {
        Quality::Low
    }
}

/// Stufe auf das Hauptbild anwenden: in der sparsamen Stufe 3D-Auflösung senken und in der
/// Umgebung der Welt SSAO und Glow abschalten. Vor dem Anlegen weiterer Ebenen aufrufen, die
/// diese Einstellungen übernehmen.
pub fn apply_to_root(root: &mut Gd<Viewport>) {
    if quality() != Quality::Low {
        return;
    }
    root.set_scaling_3d_mode(Scaling3DMode::BILINEAR);
    root.set_scaling_3d_scale(LOW_SCALE_3D);
    if let Some(mut env) = world_environment(root) {
        env.set_ssao_enabled(false);
        env.set_glow_enabled(false);
    }
}

/// Umgebung der Welt von `viewport` (aus `WorldEnvironment`), falls gesetzt.
pub fn world_environment(viewport: &Gd<Viewport>) -> Option<Gd<Environment>> {
    viewport.get_world_3d().and_then(|w| w.get_environment())
}

/// Umgebung für die Vordergrund-Ebene (Karten, Hand, Bühnen): eine Kopie von `base` ohne SSAO.
/// Die flachen Karten bekommen davon kaum etwas ab, die Ebene kostete damit aber fast so viel wie
/// das ganze Hauptbild. Glow bleibt, wie in `base` eingestellt (leuchtende Rahmen).
pub fn foreground_environment(base: &Gd<Environment>) -> Gd<Environment> {
    let mut env = base.duplicate_resource();
    env.set_ssao_enabled(false);
    env
}
