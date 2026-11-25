//! Arena-Kulisse: Sandstein-Spielbrett mit gemeißeltem, leuchtendem Runenkreis, Steinrand
//! mit Goldeinlage, Leuchtkristalle in den Ecken, gestufter Sockel, Schieferboden, Säulenruinen,
//! Feuerschalen und eine Arenamauer mit Bannern beider Seiten.
//!
//! Ersetzt die Taverne als Standard-Kulisse (`table.tscn`). Alles wird in `ready()`
//! prozedural erzeugt. Maßstab: 1 Einheit = 10 cm (eine Karte ist 0,63 × 0,88). Die
//! Brettoberfläche liegt bei y = 0, die Mittellinie bei z = 0: Spielerhälfte z > 0,
//! Gegnerhälfte z < 0.

use std::f32::consts::{FRAC_PI_2, TAU};

use godot::classes::base_material_3d::{EmissionOperator, Feature, TextureParam};
use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::light_3d::Param;
use godot::classes::{
    BoxMesh, CylinderMesh, INode3D, Material, Mesh, MeshInstance3D, PlaneMesh, SphereMesh, StandardMaterial3D,
};
use godot::prelude::*;

use super::tavern::{glow, solid, textured, FlickerLight};
use super::textures;

/// Farbe der leuchtenden Runen und Kristalle.
const ARCANE: Color = Color::from_rgb(0.35, 0.85, 1.0);
/// Höhe der Randoberkante über dem Brett (flach, damit das Brett wie eine Matte wirkt).
const RIM_TOP: f32 = 0.06;
/// Farbe der Feuerschalen.
const FIRE: Color = Color::from_rgb(1.0, 0.55, 0.2);

/// Arena-Kulisse: Sandstein-Spielbrett mit gemeißeltem, leuchtendem Runenkreis in der Mitte,
/// großer Raute und Mittellinie (ohne eingelassene Felder für Deck, Ablage und Ressourcen),
/// flacher Steinrand mit Goldeinlage, Leuchtkristalle in den Ecken,
/// gestufter Sockel, Schieferboden, Säulenruinen, Feuerschalen und Arenamauer mit Bannern.
/// Alles wird beim Start prozedural erzeugt. Maßstab: 1 Einheit = 10 cm; Brettoberfläche bei y = 0,
/// Mittellinie bei z = 0 (Spieler z > 0, Gegner z < 0).
///
/// Tool-Klasse: Die Kulisse wird auch im Godot-Editor gebaut, damit die Szene dort sichtbar ist.
/// Die erzeugten Kinder haben keinen Owner und werden daher nicht in die `.tscn` gespeichert.
/// Nach Änderungen an den Eigenschaften im Inspektor baut sich die Kulisse neu auf.
#[derive(GodotClass)]
#[class(tool, base=Node3D)]
pub struct Arena {
    /// Größe des Spielbretts (x = Breite, y = Tiefe) in Einheiten.
    #[export]
    #[var(set = set_board_size)]
    board_size: Vector2,
    /// Auflösung der Brett-Textur in Pixeln pro Einheit (höher = schärfere Ornamente, längerer Start).
    #[export(range = (20.0, 160.0))]
    #[var(set = set_texture_resolution)]
    texture_resolution: f32,
    /// Leuchtstärke des Runenkreises (0 = aus).
    #[export(range = (0.0, 8.0))]
    #[var(set = set_rune_glow)]
    rune_glow: f32,
    /// Bannerfarbe der Spielerseite.
    #[export]
    #[var(set = set_player_color)]
    player_color: Color,
    /// Bannerfarbe der Gegnerseite.
    #[export]
    #[var(set = set_opponent_color)]
    opponent_color: Color,
    /// Spielfeld aus `res://setpieces/<name>.glb` statt der prozeduralen Kulisse (aus den
    /// Blender-Dateien in `Set Pieces/`, `tools/export_setpieces.sh`): `arena`, `burg`, `ebene`,
    /// `gebirge`, `inseln`, `oedland`, `sumpf`, `taverne`, `wald`; `random` = eines davon
    /// zufällig; leer = prozedurale Kulisse. Vorrang haben das Programmargument `--board=<name>`
    /// (nach `--`) und die Umgebungsvariable `CLASH_BOARD` (so wählt NeoForge das Spielfeld).
    #[export]
    #[var(set = set_board)]
    board: GString,
    /// Neuaufbau ist bereits (verzögert) angefordert.
    rebuild_pending: bool,
    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for Arena {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            board_size: Vector2::new(14.0, 9.6),
            texture_resolution: 80.0,
            rune_glow: 1.6,
            player_color: Color::from_rgb(0.12, 0.32, 0.62),
            opponent_color: Color::from_rgb(0.55, 0.1, 0.1),
            board: GString::new(),
            rebuild_pending: false,
            base,
        }
    }

    fn ready(&mut self) {
        self.build();
    }
}

#[godot_api]
impl Arena {
    /// Baut die gesamte Kulisse neu auf (entfernt vorher alle erzeugten Kinder).
    /// Wird in `ready()` aufgerufen und im Editor automatisch nach Änderungen im Inspektor.
    #[func]
    fn rebuild(&mut self) {
        self.rebuild_pending = false;
        self.build();
    }

    /// Setter für [member board_size]; baut die Kulisse (verzögert) neu auf, wenn sie schon steht.
    #[func]
    fn set_board_size(&mut self, value: Vector2) {
        self.board_size = value;
        self.request_rebuild();
    }

    /// Setter für [member texture_resolution]; baut die Kulisse (verzögert) neu auf, wenn sie schon steht.
    #[func]
    fn set_texture_resolution(&mut self, value: f32) {
        self.texture_resolution = value;
        self.request_rebuild();
    }

    /// Setter für [member rune_glow]; baut die Kulisse (verzögert) neu auf, wenn sie schon steht.
    #[func]
    fn set_rune_glow(&mut self, value: f32) {
        self.rune_glow = value;
        self.request_rebuild();
    }

    /// Setter für [member board]; baut die Kulisse (verzögert) neu auf, wenn sie schon steht.
    #[func]
    fn set_board(&mut self, value: GString) {
        self.board = value;
        self.request_rebuild();
    }

    /// Setter für [member player_color]; baut die Kulisse (verzögert) neu auf, wenn sie schon steht.
    #[func]
    fn set_player_color(&mut self, value: Color) {
        self.player_color = value;
        self.request_rebuild();
    }

    /// Setter für [member opponent_color]; baut die Kulisse (verzögert) neu auf, wenn sie schon steht.
    #[func]
    fn set_opponent_color(&mut self, value: Color) {
        self.opponent_color = value;
        self.request_rebuild();
    }
}

impl Arena {
    /// Fordert einen Neuaufbau am Frame-Ende an (mehrere Änderungen = ein Neuaufbau).
    /// Vor `ready()` (Laden der Szene) passiert nichts – dort baut `ready()` ohnehin.
    fn request_rebuild(&mut self) {
        if self.rebuild_pending || !self.base().is_node_ready() {
            return;
        }
        self.rebuild_pending = true;
        self.base_mut().call_deferred("rebuild", &[]);
    }

    fn build(&mut self) {
        // Nur selbst erzeugte Kinder (ohne Owner) entfernen – im Editor von Hand
        // hinzugefügte Kinder bleiben erhalten.
        for mut child in self.base().get_children().iter_shared() {
            if child.get_owner().is_none() {
                self.base_mut().remove_child(&child);
                child.queue_free();
            }
        }
        // gewähltes Spielfeld (Blender-Kulisse); fehlt es, die prozedurale Kulisse
        if let Some(scene) = chosen_board(&self.board.to_string()) {
            let mut node = scene.instantiate().expect("Spielfeld instanziieren");
            tune_board_lights(&node);
            self.base_mut().add_child(&node);
            node.set_name("Spielfeld");
            super::layers::mark_background(&self.base().clone().upcast());
            return;
        }
        let m = Materials::new();
        self.build_board(&m);
        self.build_rim(&m);
        self.build_surroundings(&m);
        self.build_ruins(&m);
        self.build_wall(&m);
        // Kulisse auf den Hintergrund-Layer: die Medaillons liegen über ihr, Karten darüber
        super::layers::mark_background(&self.base().clone().upcast());
    }
}

/// Spielfelder aus `Set Pieces/` (Name = Datei `res://setpieces/<name>.glb`).
pub const BOARDS: [&str; 9] = ["arena", "burg", "ebene", "gebirge", "inseln", "oedland", "sumpf", "taverne", "wald"];

/// Gewähltes Spielfeld: Argument `--board=`, sonst `CLASH_BOARD`, sonst `property`; `random` =
/// eines der vorhandenen zufällig. None = prozedurale Kulisse (leer, unbekannt oder Datei fehlt).
fn chosen_board(property: &str) -> Option<Gd<godot::classes::PackedScene>> {
    use godot::classes::{Os, ResourceLoader};
    use godot::obj::Singleton;
    let args = Os::singleton().get_cmdline_user_args();
    let from_arg = args.as_slice().iter().find_map(|a| a.to_string().strip_prefix("--board=").map(str::to_string));
    let name = from_arg
        .or_else(|| std::env::var("CLASH_BOARD").ok())
        .unwrap_or_else(|| property.to_string())
        .trim()
        .to_ascii_lowercase();
    if name.is_empty() {
        return None;
    }
    let path = |n: &str| format!("res://setpieces/{n}.glb");
    let mut loader = ResourceLoader::singleton();
    let name = if name == "random" {
        let have: Vec<&str> = BOARDS.iter().copied().filter(|n| loader.exists(&path(n))).collect();
        if have.is_empty() {
            return None;
        }
        let i = (godot::global::randi() as usize) % have.len();
        have[i].to_string()
    } else {
        name
    };
    if !BOARDS.contains(&name.as_str()) || !loader.exists(&path(&name)) {
        godot_warn!("Spielfeld „{name}“ nicht vorhanden, prozedurale Kulisse");
        return None;
    }
    let scene = loader.load(&path(&name)).and_then(|r| r.try_cast::<godot::classes::PackedScene>().ok());
    if scene.is_some() {
        godot_print!("Spielfeld: {name}");
    }
    scene
}

/// Lichter eines Spielfelds für den Tisch einstellen: ohne Schatten (die Karten wirft das
/// Tischlicht) und Helligkeit wie in Blender gedacht. Godots glTF-Import macht aus Blenders Watt
/// sehr hohe Energiewerte (≈ 54 je Watt) mit Reichweite 4096; hier wird daraus Energie ≈ Watt/60
/// und eine Reichweite nach der Stärke (Kerze kurz, Kaminfeuer weit).
fn tune_board_lights(node: &Gd<Node>) {
    use godot::classes::light_3d::Param;
    use godot::classes::{Light3D, OmniLight3D};
    for child in node.get_children().iter_shared() {
        if let Ok(mut l) = child.clone().try_cast::<Light3D>() {
            l.set_shadow(false);
            let watts = l.get_param(Param::ENERGY) / GLTF_ENERGY_PER_WATT;
            match l.clone().try_cast::<OmniLight3D>() {
                Ok(mut o) => {
                    o.set_param(Param::ENERGY, (watts / WATTS_PER_ENERGY).clamp(0.05, 3.0));
                    o.set_param(Param::RANGE, (2.5 + watts.sqrt() * 0.7).clamp(2.0, 10.0));
                }
                // Hauptlicht (in Blender eine Fläche über dem Tisch, als Spot exportiert)
                Err(_) => {
                    // ohne Abfall mit der Entfernung, wie eine große Leuchtfläche über dem Tisch
                    l.set_param(Param::ENERGY, (watts / MAIN_WATTS_PER_ENERGY).clamp(0.1, 3.0));
                    l.set_param(Param::RANGE, 60.0);
                    l.set_param(Param::ATTENUATION, 0.0);
                }
            }
        }
        tune_board_lights(&child);
    }
}

/// Energie je Blender-Watt nach Godots glTF-Import (gemessen: 80 W → 4348).
const GLTF_ENERGY_PER_WATT: f32 = 54.35;
/// So viele Blender-Watt ergeben Energie 1 am Tisch (Punktlichter).
const WATTS_PER_ENERGY: f32 = 60.0;
/// Dasselbe für das Hauptlicht (Spot aus Blenders Flächenlicht, weit über dem Tisch).
const MAIN_WATTS_PER_ENERGY: f32 = 4000.0;

/// Gemeinsam genutzte Materialien (Stein triplanar in Weltkoordinaten projiziert).
struct Materials {
    rim_stone: Gd<Material>,
    plinth: Gd<Material>,
    floor: Gd<Material>,
    column: Gd<Material>,
    wall: Gd<Material>,
    gold: Gd<Material>,
    iron: Gd<Material>,
    crystal: Gd<Material>,
    fire: Gd<Material>,
}

impl Materials {
    fn new() -> Self {
        let stone = textures::stone_wall();
        let tint = |m: Gd<Material>, c: Color| {
            let mut s = m.cast::<StandardMaterial3D>();
            s.set_albedo(c);
            s.upcast::<Material>()
        };
        Self {
            rim_stone: tint(textured(&stone, 0.3, 0.9), Color::from_rgb(0.85, 0.8, 0.72)),
            plinth: tint(textured(&textures::flagstones(), 0.07, 0.9), Color::from_rgb(1.15, 1.08, 0.98)),
            floor: textured(&textures::flagstones(), 0.035, 0.85),
            column: tint(textured(&stone, 0.15, 0.9), Color::from_rgb(0.85, 0.8, 0.72)),
            wall: tint(textured(&stone, 0.06, 0.95), Color::from_rgb(0.55, 0.53, 0.52)),
            gold: solid(Color::from_rgb(0.85, 0.66, 0.3), 0.3, 1.0),
            iron: solid(Color::from_rgb(0.1, 0.1, 0.11), 0.5, 0.8),
            crystal: glow(ARCANE, 1.4),
            fire: glow(FIRE, 4.0),
        }
    }
}

impl Arena {
    fn add_mesh<T: Inherits<Mesh>>(&mut self, name: &str, mesh: Gd<T>, pos: Vector3) -> Gd<MeshInstance3D> {
        let mut mi = MeshInstance3D::new_alloc();
        mi.set_name(name);
        mi.set_mesh(&mesh.upcast::<Mesh>());
        mi.set_position(pos);
        self.base_mut().add_child(&mi);
        mi
    }

    fn add_box(&mut self, name: &str, size: Vector3, pos: Vector3, mat: &Gd<Material>) -> Gd<MeshInstance3D> {
        let mut b = BoxMesh::new_gd();
        b.set_size(size);
        b.set_material(mat);
        self.add_mesh(name, b, pos)
    }

    fn add_cylinder(
        &mut self,
        name: &str,
        radius: (f32, f32),
        height: f32,
        pos: Vector3,
        mat: &Gd<Material>,
    ) -> Gd<MeshInstance3D> {
        let mut c = CylinderMesh::new_gd();
        c.set_top_radius(radius.0);
        c.set_bottom_radius(radius.1);
        c.set_height(height);
        c.set_material(mat);
        self.add_mesh(name, c, pos)
    }

    fn add_light(&mut self, pos: Vector3, color: Color, energy: f32, range: f32, strength: f32) {
        let mut light = FlickerLight::new_alloc();
        light.set_color(color);
        light.set_param(Param::RANGE, range);
        light.set_param(Param::ATTENUATION, 1.3);
        light.set_shadow(false);
        light.set_position(pos);
        light.bind_mut().configure(energy, strength);
        self.base_mut().add_child(&light);
    }

    // ------------------------------------------------------------ Spielbrett

    fn build_board(&mut self, m: &Materials) {
        let bs = self.board_size;
        let tex = textures::arena_board(bs, self.texture_resolution);

        let mut mat = StandardMaterial3D::new_gd();
        mat.set_texture(TextureParam::ALBEDO, &tex.albedo);
        mat.set_feature(Feature::NORMAL_MAPPING, true);
        mat.set_texture(TextureParam::NORMAL, &tex.normal);
        mat.set_normal_scale(1.2);
        mat.set_feature(Feature::EMISSION, true);
        mat.set_texture(TextureParam::EMISSION, &tex.emission);
        // MULTIPLY: Leuchten = Farbe × Textur (ADD würde die Farbe auf das ganze Brett legen)
        mat.set_emission(Color::from_rgb(1.0, 1.0, 1.0));
        mat.set_emission_operator(EmissionOperator::MULTIPLY);
        mat.set_emission_energy_multiplier(self.rune_glow);
        mat.set_roughness(0.9);

        // Steinkörper + texturierte Oberfläche (PlaneMesh: u entlang +x, v entlang +z)
        self.add_box("BoardBody", Vector3::new(bs.x, 0.8, bs.y), Vector3::new(0.0, -0.41, 0.0), &m.rim_stone);
        let mut top = PlaneMesh::new_gd();
        top.set_size(bs);
        top.set_material(&mat);
        self.add_mesh("BoardTop", top, Vector3::ZERO);

        // Sanftes Leuchten über dem Runenkreis
        if self.rune_glow > 0.0 {
            self.add_light(Vector3::new(0.0, 1.2, 0.0), ARCANE, 0.5 * self.rune_glow, 4.0, 0.08);
        }
    }

    /// Flacher Steinrand mit Goldeinlage, Eckblöcke mit eingelassenem Leuchtkristall.
    fn build_rim(&mut self, m: &Materials) {
        let bs = self.board_size;
        let rim = 0.6;
        // Flach wie eine Spielmatte: Rand nur knapp über dem Brett
        let (h, y) = (0.6 + RIM_TOP, (RIM_TOP - 0.6) * 0.5);
        for (size, pos) in [
            (Vector3::new(bs.x + rim * 2.0, h, rim), Vector3::new(0.0, y, bs.y * 0.5 + rim * 0.5)),
            (Vector3::new(bs.x + rim * 2.0, h, rim), Vector3::new(0.0, y, -bs.y * 0.5 - rim * 0.5)),
            (Vector3::new(rim, h, bs.y), Vector3::new(bs.x * 0.5 + rim * 0.5, y, 0.0)),
            (Vector3::new(rim, h, bs.y), Vector3::new(-bs.x * 0.5 - rim * 0.5, y, 0.0)),
        ] {
            self.add_box("Rim", size, pos, &m.rim_stone);
        }
        // Goldeinlage an der Innenkante
        let gy = RIM_TOP + 0.005;
        for (size, pos) in [
            (Vector3::new(bs.x + 0.12, 0.03, 0.06), Vector3::new(0.0, gy, bs.y * 0.5 + 0.03)),
            (Vector3::new(bs.x + 0.12, 0.03, 0.06), Vector3::new(0.0, gy, -bs.y * 0.5 - 0.03)),
            (Vector3::new(0.06, 0.03, bs.y), Vector3::new(bs.x * 0.5 + 0.03, gy, 0.0)),
            (Vector3::new(0.06, 0.03, bs.y), Vector3::new(-bs.x * 0.5 - 0.03, gy, 0.0)),
        ] {
            let mut g = self.add_box("GoldInlay", size, pos, &m.gold);
            g.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        }
        // Mittelmarken am Rand (Verlängerung der Mittellinie)
        for x in [-1.0f32, 1.0] {
            let mut g = self.add_box(
                "GoldMark",
                Vector3::new(rim * 0.8, 0.04, 0.12),
                Vector3::new(x * (bs.x * 0.5 + rim * 0.5), gy, 0.0),
                &m.gold,
            );
            g.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        }

        // Flache Eckblöcke mit Goldfassung und eingelassenem, leuchtendem Kristall
        let cx = bs.x * 0.5 + rim * 0.5;
        let cz = bs.y * 0.5 + rim * 0.5;
        for (sx, sz) in [(-1.0f32, -1.0f32), (1.0, -1.0), (-1.0, 1.0), (1.0, 1.0)] {
            let base = Vector3::new(sx * cx, 0.0, sz * cz);
            self.add_box("CornerBlock", Vector3::new(1.1, 0.6, 1.1), base + Vector3::new(0.0, RIM_TOP - 0.3, 0.0), &m.rim_stone);
            self.add_cylinder("CornerSetting", (0.36, 0.4), 0.04, base + Vector3::new(0.0, RIM_TOP + 0.02, 0.0), &m.gold);
            let mut gem = SphereMesh::new_gd();
            gem.set_radius(0.16);
            gem.set_height(0.08);
            gem.set_radial_segments(6);
            gem.set_rings(2);
            gem.set_material(&m.crystal);
            let mut g = self.add_mesh("Crystal", gem, base + Vector3::new(0.0, RIM_TOP + 0.05, 0.0));
            g.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        }
    }

    // ------------------------------------------------------------ Umgebung

    /// Gestufter Sockel, Schieferboden und zwei Feuerschalen neben dem Brett.
    fn build_surroundings(&mut self, m: &Materials) {
        let bs = self.board_size;
        self.add_box("PlinthTop", Vector3::new(bs.x + 4.0, 0.6, bs.y + 4.0), Vector3::new(0.0, -0.8, 0.0), &m.plinth);
        self.add_box("PlinthStep", Vector3::new(bs.x + 8.0, 0.6, bs.y + 8.0), Vector3::new(0.0, -1.4, 0.0), &m.plinth);
        let mut floor = PlaneMesh::new_gd();
        floor.set_size(Vector2::new(160.0, 160.0));
        floor.set_material(&m.floor);
        self.add_mesh("Floor", floor, Vector3::new(0.0, -1.7, 0.0));

        for x in [-1.0f32, 1.0] {
            let pos = Vector3::new(x * (bs.x * 0.5 + 5.5), -1.7, -1.0);
            self.add_cylinder("BrazierStand", (0.25, 0.5), 2.6, pos + Vector3::new(0.0, 1.3, 0.0), &m.iron);
            self.add_cylinder("BrazierBowl", (0.9, 0.45), 0.5, pos + Vector3::new(0.0, 2.85, 0.0), &m.iron);
            let mut embers = self.add_cylinder("Embers", (0.8, 0.8), 0.1, pos + Vector3::new(0.0, 3.05, 0.0), &m.fire);
            embers.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            for (dx, dz, hh) in [(0.0, 0.0, 1.1), (0.3, 0.15, 0.7), (-0.28, -0.1, 0.8)] {
                let mut flame = CylinderMesh::new_gd();
                flame.set_top_radius(0.0);
                flame.set_bottom_radius(0.22);
                flame.set_height(hh);
                flame.set_material(&m.fire);
                let mut f = self.add_mesh("Flame", flame, pos + Vector3::new(dx, 3.1 + hh * 0.5, dz));
                f.set_cast_shadows_setting(ShadowCastingSetting::OFF);
            }
            self.add_light(pos + Vector3::new(0.0, 4.2, 0.0), FIRE, 4.0, 16.0, 0.3);
        }
    }

    /// Ring aus teils zerbrochenen Säulen um den Sockel (nicht vor der Kamera).
    fn build_ruins(&mut self, m: &Materials) {
        let fy = -1.7;
        let count = 14;
        for i in 0..count {
            let a = i as f32 / count as f32 * TAU;
            let (rx, rz) = (18.0, 14.0);
            let (x, z) = (a.cos() * rx, a.sin() * rz - 2.0);
            if z > 6.0 {
                continue; // hinter der Kamera bzw. vor dem Spieler
            }
            let k = (i * 7 + 3) % 5;
            let broken = k == 1 || k == 3;
            let height = if broken { 2.5 + k as f32 * 0.8 } else { 9.0 };
            self.add_box("ColumnBase", Vector3::new(2.4, 0.6, 2.4), Vector3::new(x, fy + 0.3, z), &m.column);
            self.add_cylinder("Column", (0.85, 0.95), height, Vector3::new(x, fy + 0.6 + height * 0.5, z), &m.column);
            if broken {
                // Herabgefallenes Säulenstück daneben
                let mut piece = self.add_cylinder(
                    "ColumnPiece",
                    (0.85, 0.85),
                    2.6,
                    Vector3::new(x + 1.6, fy + 0.85, z + 0.8),
                    &m.column,
                );
                piece.set_rotation(Vector3::new(0.0, a, FRAC_PI_2));
            } else {
                self.add_box("Capital", Vector3::new(2.2, 0.7, 2.2), Vector3::new(x, fy + 0.6 + height + 0.35, z), &m.column);
            }
        }
    }

    /// Runde Arenamauer mit Bannern: links/oben Gegnerfarbe, rechts Spielerfarbe abwechselnd.
    fn build_wall(&mut self, m: &Materials) {
        let fy = -1.7;
        let (radius, height) = (30.0f32, 12.0f32);
        let segments = 40;
        let seg_w = TAU * radius / segments as f32 + 0.3;
        for i in 0..segments {
            let a = i as f32 / segments as f32 * TAU;
            let (x, z) = (a.cos() * radius, a.sin() * radius);
            if z > 12.0 {
                continue;
            }
            let mut wall = self.add_box("ArenaWall", Vector3::new(seg_w, height, 1.5), Vector3::new(x, fy + height * 0.5, z), &m.wall);
            // Box-Längsseite tangential zur Mauer ausrichten
            wall.set_rotation(Vector3::new(0.0, -a - FRAC_PI_2, 0.0));
            if i % 4 == 0 {
                let color = if i % 8 == 0 { self.opponent_color } else { self.player_color };
                let inner = Vector3::new(a.cos(), 0.0, a.sin()) * (radius - 0.85);
                let mut banner = self.add_box(
                    "Banner",
                    Vector3::new(2.4, 6.5, 0.08),
                    inner + Vector3::new(0.0, fy + height - 4.2, 0.0),
                    &solid(color, 0.85, 0.0),
                );
                banner.set_rotation(Vector3::new(0.0, -a - FRAC_PI_2, 0.0));
                let mut rod = self.add_box(
                    "BannerRod",
                    Vector3::new(2.8, 0.15, 0.15),
                    inner + Vector3::new(0.0, fy + height - 0.9, 0.0),
                    &m.gold,
                );
                rod.set_rotation(Vector3::new(0.0, -a - FRAC_PI_2, 0.0));
            }
        }
    }
}
