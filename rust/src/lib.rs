use godot::prelude::*;

mod assets;
mod hello;
#[macro_use]
mod i18n;
mod forge;
mod game;
mod rules;
mod table;

struct MtgClashExtension;

#[gdextension]
unsafe impl ExtensionLibrary for MtgClashExtension {
    /// Vor dem Herunterfahren: geteilte Zwischenspeicher leeren. Auch der Editor und Godots
    /// Abfrage von Standardwerten erzeugen Klassen (z. B. `CardCutout`, `TargetArrows` legen ihren
    /// Shader schon in `init` an); ohne das meldete Godot beim Beenden übrige Shader.
    fn on_stage_deinit(stage: godot::init::InitStage) {
        if stage == godot::init::InitStage::MainLoop {
            table::shared::clear();
            table::placeholder::clear_printed();
            game::mana_symbols::clear();
            table::dungeon_view::clear();
        }
    }
}
