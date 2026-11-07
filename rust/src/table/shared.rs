//! Gemeinsam genutzte Ressourcen: Shader mit gleichem Quelltext und gleiche erzeugte Bilder gibt
//! es nur einmal, statt je Karte, Knopf oder Effekt neu. Das spart das Übersetzen der Shader (kein
//! Ruckeln, wenn neue Karten oder Effekte erscheinen), Grafikspeicher und Rechenzeit beim Erzeugen.
//! Parameter bleiben je Material (`ShaderMaterial` teilt nur den Shader).

use std::cell::RefCell;
use std::collections::HashMap;

use godot::classes::{Shader, Texture2D};
use godot::prelude::*;

thread_local! {
    static SHADERS: RefCell<HashMap<String, Gd<Shader>>> = RefCell::new(HashMap::new());
    static TEXTURES: RefCell<HashMap<&'static str, Gd<Texture2D>>> = RefCell::new(HashMap::new());
}

/// Shader zum Quelltext `code`, je Quelltext nur einmal angelegt.
pub fn shader(code: &str) -> Gd<Shader> {
    if let Some(s) = SHADERS.with(|m| m.borrow().get(code).cloned()) {
        return s;
    }
    let mut s = Shader::new_gd();
    s.set_code(code);
    SHADERS.with(|m| m.borrow_mut().insert(code.to_owned(), s.clone()));
    s
}

/// Erzeugtes Bild unter dem Schlüssel `key`: beim ersten Aufruf mit `make` erzeugt, danach geteilt.
pub fn texture(key: &'static str, make: impl FnOnce() -> Gd<Texture2D>) -> Gd<Texture2D> {
    if let Some(t) = TEXTURES.with(|m| m.borrow().get(key).cloned()) {
        return t;
    }
    let t = make();
    TEXTURES.with(|m| m.borrow_mut().insert(key, t.clone()));
    t
}

/// Zwischenspeicher leeren (beim Abbau des Tisches, damit beim Beenden keine Ressourcen übrig bleiben).
pub fn clear() {
    SHADERS.with(|m| m.borrow_mut().clear());
    TEXTURES.with(|m| m.borrow_mut().clear());
}
