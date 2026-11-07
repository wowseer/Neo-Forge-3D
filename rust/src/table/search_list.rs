//! `SearchList`: Auswahlliste mit Filterfeld in der 3D-Szene (Baustein der 3D-Oberfläche, z. B.
//! für „Wähle einen Kreaturtyp“ und „Nenne einen Kartennamen“).
//!
//! Aufbau (Bühnen-Einheiten, Mitte der Liste bei x = 0, liegt in der Ebene der Bühne):
//! - **Filterfeld**: abgerundete dunkle Fläche mit Eingabetext (bzw. „Filter …“) und Lupe rechts,
//! - **Liste**: abgerundete, halbtransparente Fläche mit [`ROWS`] sichtbaren Zeilen; die Zeile
//!   unter der Maus hellt auf, die gewählte leuchtet golden,
//! - **Scrollbalken** rechts in der Liste (nur bei mehr Einträgen als Zeilen).
//!
//! Die Liste zeigt immer nur die sichtbaren Zeilen (beliebig viele Einträge, z. B. alle
//! Kartennamen). Bedienung: Der [`TableController`](super::table_controller::TableController)
//! trifft die Kollisionsfläche (Layer [`CONTROL_LAYER`]) per Raycast, meldet den Trefferpunkt
//! ([`SearchList::update_pointer`]) und löst beim Klick [`SearchList::activate`] aus: Klick auf
//! eine Zeile wählt sie (Signal `selected`), ein zweiter Klick auf dieselbe Zeile bestätigt
//! (Signal `confirmed`), Klick in den Scrollbalken springt dorthin. Das Mausrad scrollt, solange
//! die Maus über der Liste liegt.
//!
//! Zeiten nach `docs/claude/ANIMATION.md`: Erscheinen 0,20 s Cubic-Out (wächst von 96 % und
//! blendet ein), Zeilen-Hover exponentiell (18/s).

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::label_3d::DrawFlags;
use godot::classes::{
    Area3D, BoxShape3D, CollisionShape3D, INode3D, InputEvent, InputEventMouseButton, Label3D, MeshInstance3D,
    QuadMesh, ShaderMaterial,
};
use godot::global::{HorizontalAlignment, MouseButton};
use godot::prelude::*;

use super::layers::CONTROL_LAYER;

/// Sichtbare Zeilen, Zeilenhöhe, Breite der Liste und Innenabstand (Bühnen-Einheiten).
pub const ROWS: usize = 7;
const ROW_H: f32 = 0.118;
pub const WIDTH: f32 = 1.55;
const PAD: f32 = 0.035;
/// Filterfeld: Höhe und Abstand zur Liste.
const FIELD_H: f32 = 0.15;
const FIELD_GAP: f32 = 0.035;
/// Scrollbalken: Breite und Abstand vom rechten Rand.
const BAR_W: f32 = 0.032;
const BAR_MARGIN: f32 = 0.028;
/// Schrift der Zeilen und des Filterfelds.
const FONT: i32 = 64;
const PIXEL: f32 = 0.00118;
const APPEAR_TIME: f32 = 0.20;
const HOVER_SPEED: f32 = 18.0;
/// Zwei Klicks auf dieselbe Zeile innerhalb dieser Zeit bestätigen (Sekunden).
const DOUBLE_CLICK: f64 = 0.45;

const TEXT: Color = Color::from_rgb(0.93, 0.93, 0.95);
const TEXT_SELECTED: Color = Color::from_rgb(1.0, 0.86, 0.5);
const PLACEHOLDER: Color = Color::from_rgb(0.55, 0.55, 0.6);

/// Abgerundetes Rechteck (SDF) mit Füllung, Rand und auf Wunsch einer Lupe rechts.
const SHADER: &str = r#"
shader_type spatial;
render_mode unshaded, cull_disabled, depth_draw_never, shadows_disabled;
uniform vec2 size = vec2(1.0, 0.2);
uniform float radius = 0.05;
uniform vec4 fill : source_color = vec4(0.0, 0.0, 0.0, 0.8);
uniform vec4 border : source_color = vec4(1.0, 1.0, 1.0, 0.0);
uniform float border_w = 0.0;
uniform float icon = 0.0;
uniform float fade = 1.0;
float box(vec2 p, vec2 b, float r) {
    vec2 q = abs(p) - b + r;
    return length(max(q, 0.0)) + min(max(q.x, q.y), 0.0) - r;
}
void fragment() {
    vec2 p = (UV - 0.5) * size;
    p.y = -p.y;
    float d = box(p, size * 0.5, radius);
    float aa = max(fwidth(d), 1e-5);
    float inside = 1.0 - smoothstep(-aa, aa, d);
    vec4 c = fill;
    if (border_w > 0.0) {
        c = mix(fill, border, smoothstep(-border_w - aa, -border_w + aa, d));
    }
    if (icon > 0.5) {
        // Lupe: Ring oben links, Griff nach rechts unten
        float s = size.y;
        vec2 q = p - vec2(size.x * 0.5 - s * 0.55, 0.0);
        float ring = abs(length(q - vec2(-0.05, 0.05) * s) - 0.17 * s) - 0.03 * s;
        vec2 dir = normalize(vec2(1.0, -1.0));
        vec2 h0 = vec2(0.07, -0.07) * s;
        float t = clamp(dot(q - h0, dir), 0.0, 0.17 * s);
        float handle = length(q - h0 - dir * t) - 0.035 * s;
        float g = min(ring, handle);
        c = mix(c, vec4(0.72, 0.72, 0.76, 1.0), 1.0 - smoothstep(-aa, aa, g));
    }
    ALBEDO = c.rgb;
    ALPHA = c.a * inside * fade;
}
"#;

/// Auswahlliste mit Filterfeld: zeigt [method set_entries]-Einträge in sieben Zeilen, scrollt
/// mit dem Mausrad bzw. Scrollbalken. Signale [signal selected] (Zeile angeklickt) und
/// [signal confirmed] (zweiter Klick auf die gewählte Zeile).
#[derive(GodotClass)]
#[class(base=Node3D)]
pub struct SearchList {
    entries: Vec<String>,
    /// Erste sichtbare Zeile.
    offset: usize,
    selected: Option<usize>,
    /// Zeile unter der Maus (Nummer des Eintrags) bzw. Maus über dem Scrollbalken.
    hovered_row: Option<usize>,
    over_bar: bool,
    pointer_y: f32,
    inside: bool,
    last_click: Option<(usize, f64)>,
    clock: f64,
    appear: f32,

    labels: Vec<Gd<Label3D>>,
    row_hover: Vec<f32>,
    hover_quad: Option<(Gd<MeshInstance3D>, Gd<ShaderMaterial>)>,
    select_quad: Option<(Gd<MeshInstance3D>, Gd<ShaderMaterial>)>,
    thumb: Option<(Gd<MeshInstance3D>, Gd<ShaderMaterial>)>,
    track: Option<(Gd<MeshInstance3D>, Gd<ShaderMaterial>)>,
    field_label: Option<Gd<Label3D>>,
    materials: Vec<Gd<ShaderMaterial>>,

    base: Base<Node3D>,
}

#[godot_api]
impl INode3D for SearchList {
    fn init(base: Base<Node3D>) -> Self {
        Self {
            entries: Vec::new(),
            offset: 0,
            selected: None,
            hovered_row: None,
            over_bar: false,
            pointer_y: 0.0,
            inside: false,
            last_click: None,
            clock: 0.0,
            appear: 0.0,
            labels: Vec::new(),
            row_hover: vec![0.0; ROWS],
            hover_quad: None,
            select_quad: None,
            thumb: None,
            track: None,
            field_label: None,
            materials: Vec::new(),
            base,
        }
    }

    fn ready(&mut self) {
        self.build();
        self.refresh();
    }

    fn process(&mut self, delta: f64) {
        self.clock += delta;
        let dt = delta as f32;
        self.appear = (self.appear + dt).min(APPEAR_TIME);
        let p = self.appear / APPEAR_TIME;
        let ease = 1.0 - (1.0 - p).powi(3);
        self.base_mut().set_scale(Vector3::splat(0.96 + 0.04 * ease));
        for m in self.materials.iter_mut() {
            m.set_shader_parameter("fade", &ease.to_variant());
        }
        // Zeilen-Hover weich; Hover-Fläche folgt der Zeile unter der Maus
        let hovered_slot = self.hovered_row.and_then(|r| r.checked_sub(self.offset)).filter(|s| *s < ROWS);
        let k = 1.0 - (-HOVER_SPEED * dt).exp();
        for (slot, h) in self.row_hover.iter_mut().enumerate() {
            let target = if hovered_slot == Some(slot) { 1.0 } else { 0.0 };
            *h += (target - *h) * k;
        }
        let amount = hovered_slot.map_or(0.0, |s| self.row_hover[s]);
        if let Some((quad, mat)) = self.hover_quad.as_mut() {
            quad.set_visible(hovered_slot.is_some() && hovered_slot.map(|s| s + self.offset) != self.selected);
            if let Some(s) = hovered_slot {
                quad.set_position(Vector3::new(-BAR_W * 0.5 - BAR_MARGIN * 0.5, row_y(s), 0.004));
            }
            mat.set_shader_parameter("fill", &Color::from_rgba(1.0, 1.0, 1.0, 0.09 * amount).to_variant());
        }
        for (slot, l) in self.labels.iter_mut().enumerate() {
            let a = ease;
            let selected = self.selected == Some(slot + self.offset);
            let base = if selected { TEXT_SELECTED } else { TEXT };
            let lift = if selected { 0.0 } else { 0.07 * self.row_hover[slot] };
            l.set_modulate(Color::from_rgba(
                (base.r + lift).min(1.0),
                (base.g + lift).min(1.0),
                (base.b + lift).min(1.0),
                a,
            ));
            l.set_outline_modulate(Color::from_rgba(0.0, 0.0, 0.0, 0.6 * a));
        }
    }

    /// Mausrad über der Liste: scrollen (eine Zeile je Raste); das Ereignis geht nicht an die Kamera.
    fn input(&mut self, event: Gd<InputEvent>) {
        if !self.inside {
            return;
        }
        let Ok(mb) = event.try_cast::<InputEventMouseButton>() else { return };
        if !mb.is_pressed() {
            return;
        }
        let step: i64 = match mb.get_button_index() {
            MouseButton::WHEEL_UP => -1,
            MouseButton::WHEEL_DOWN => 1,
            _ => return,
        };
        self.scroll_by(step);
        if let Some(mut vp) = self.base().get_viewport() {
            vp.set_input_as_handled();
        }
    }
}

#[godot_api]
impl SearchList {
    /// Eintrag `index` (Nummer in der Liste von [method set_entries]) wurde angeklickt.
    #[signal]
    fn selected(index: i64);

    /// Die gewählte Zeile wurde noch einmal angeklickt (Doppelklick): Auswahl bestätigen.
    #[signal]
    fn confirmed(index: i64);

    /// Neue Einträge (z. B. nach geänderter Eingabe): Liste springt nach oben, Auswahl bleibt,
    /// wenn `keep` (Nummer in der neuen Liste) gesetzt ist.
    pub fn set_entries(&mut self, entries: Vec<String>, keep: Option<usize>) {
        self.entries = entries;
        self.selected = keep.filter(|i| *i < self.entries.len());
        self.offset = 0;
        if let Some(s) = self.selected {
            self.scroll_into_view(s);
        }
        self.hovered_row = None;
        self.refresh();
    }

    /// Gewählter Eintrag (Nummer in der Liste von [method set_entries]), -1 = keiner.
    #[func]
    pub fn selected_index(&self) -> i64 {
        self.selected.map_or(-1, |i| i as i64)
    }

    /// Eintrag unter der Maus, -1 = keiner.
    #[func]
    pub fn hovered_index(&self) -> i64 {
        self.hovered_row.map_or(-1, |i| i as i64)
    }

    /// Auswahl um `step` Zeilen verschieben (Pfeiltasten); ohne Auswahl die erste Zeile.
    #[func]
    pub fn move_selection(&mut self, step: i64) {
        if self.entries.is_empty() {
            return;
        }
        let last = self.entries.len() as i64 - 1;
        let next = match self.selected {
            None => 0,
            Some(i) => (i as i64 + step).clamp(0, last),
        } as usize;
        self.selected = Some(next);
        self.scroll_into_view(next);
        self.refresh();
        self.signals().selected().emit(next as i64);
    }

    /// Text im Filterfeld: leer zeigt den Platzhalter `placeholder`.
    pub fn set_query(&mut self, query: &str, placeholder: &str) {
        if let Some(l) = self.field_label.as_mut() {
            if query.is_empty() {
                l.set_text(placeholder);
                l.set_modulate(PLACEHOLDER);
            } else {
                l.set_text(&format!("{query}▌"));
                l.set_modulate(TEXT);
            }
        }
    }

    /// Unterkante der Liste (Bühnen-Einheiten, relativ zur Mitte der Liste), für Knöpfe darunter.
    pub fn bottom() -> f32 {
        -(list_height() * 0.5)
    }

    /// Mauspunkt auf der Trefferfläche (Weltkoordinaten): Zeile bzw. Scrollbalken darunter.
    #[func]
    pub fn update_pointer(&mut self, world: Vector3) {
        let local = self.base().get_global_transform().affine_inverse() * world;
        self.inside = true;
        self.pointer_y = local.y;
        let half_h = list_height() * 0.5;
        let in_list = local.x.abs() <= WIDTH * 0.5 && local.y.abs() <= half_h;
        self.over_bar = in_list && self.entries.len() > ROWS && local.x >= WIDTH * 0.5 - BAR_MARGIN - BAR_W * 1.5;
        self.hovered_row = if in_list && !self.over_bar {
            let from_top = half_h - PAD - local.y;
            let slot = (from_top / ROW_H).floor();
            (slot >= 0.0 && (slot as usize) < ROWS)
                .then(|| slot as usize + self.offset)
                .filter(|i| *i < self.entries.len())
        } else {
            None
        };
    }

    /// Maus verlässt die Liste.
    #[func]
    pub fn set_hovered(&mut self, hovered: bool) {
        if !hovered {
            self.inside = false;
            self.hovered_row = None;
            self.over_bar = false;
        }
    }

    /// Klick: Zeile wählen (zweimal = bestätigen) bzw. im Scrollbalken dorthin springen.
    #[func]
    pub fn activate(&mut self) {
        if self.over_bar {
            let half = list_height() * 0.5 - PAD;
            let frac = ((half - self.pointer_y) / (2.0 * half)).clamp(0.0, 1.0);
            let max = self.entries.len().saturating_sub(ROWS);
            self.offset = (frac * max as f32).round() as usize;
            self.refresh();
            return;
        }
        let Some(i) = self.hovered_row else { return };
        let double = self.last_click.is_some_and(|(j, t)| j == i && self.clock - t <= DOUBLE_CLICK);
        self.last_click = Some((i, self.clock));
        if double && self.selected == Some(i) {
            self.signals().confirmed().emit(i as i64);
            return;
        }
        self.selected = Some(i);
        self.refresh();
        self.signals().selected().emit(i as i64);
    }
}

impl SearchList {
    fn scroll_by(&mut self, step: i64) {
        let max = self.entries.len().saturating_sub(ROWS) as i64;
        let next = (self.offset as i64 + step).clamp(0, max) as usize;
        if next != self.offset {
            // Hover-Zeile wandert mit dem Inhalt
            if let Some(r) = self.hovered_row {
                let slot = r as i64 - self.offset as i64;
                self.hovered_row = usize::try_from(next as i64 + slot).ok().filter(|i| *i < self.entries.len());
            }
            self.offset = next;
            self.refresh();
        }
    }

    fn scroll_into_view(&mut self, i: usize) {
        if i < self.offset {
            self.offset = i;
        } else if i >= self.offset + ROWS {
            self.offset = i + 1 - ROWS;
        }
    }

    /// Sichtbare Zeilen, Auswahl-Fläche und Scrollbalken nach Einträgen und Lage neu setzen.
    fn refresh(&mut self) {
        for (slot, l) in self.labels.iter_mut().enumerate() {
            let text = self.entries.get(self.offset + slot).map(|t| clip(t)).unwrap_or_default();
            l.set_text(&text);
        }
        let sel_slot = self.selected.and_then(|s| s.checked_sub(self.offset)).filter(|s| *s < ROWS);
        if let Some((quad, _)) = self.select_quad.as_mut() {
            quad.set_visible(sel_slot.is_some());
            if let Some(s) = sel_slot {
                quad.set_position(Vector3::new(-BAR_W * 0.5 - BAR_MARGIN * 0.5, row_y(s), 0.005));
            }
        }
        let scroll = self.entries.len() > ROWS;
        let track_h = list_height() - 2.0 * PAD;
        if let Some((t, _)) = self.track.as_mut() {
            t.set_visible(scroll);
        }
        if let Some((thumb, mat)) = self.thumb.as_mut() {
            thumb.set_visible(scroll);
            if scroll {
                let n = self.entries.len() as f32;
                let h = (track_h * ROWS as f32 / n).max(0.09);
                let max = (n - ROWS as f32).max(1.0);
                let frac = self.offset as f32 / max;
                let y = track_h * 0.5 - h * 0.5 - frac * (track_h - h);
                thumb.set_scale(Vector3::new(1.0, h, 1.0));
                thumb.set_position(Vector3::new(bar_x(), y, 0.006));
                mat.set_shader_parameter("size", &Vector2::new(BAR_W, h).to_variant());
            }
        }
    }

    fn build(&mut self) {
        let h = list_height();
        // Liste
        self.panel(Vector2::new(WIDTH, h), Vector3::ZERO, Color::from_rgba(0.06, 0.06, 0.08, 0.94), 0.06, false, true);
        // Filterfeld darüber
        let field_y = h * 0.5 + FIELD_GAP + FIELD_H * 0.5;
        self.panel(Vector2::new(WIDTH, FIELD_H), Vector3::new(0.0, field_y, 0.0), Color::from_rgba(0.0, 0.0, 0.0, 0.92), FIELD_H * 0.5, true, true);
        let mut field = self.text_label(-WIDTH * 0.5 + 0.07, field_y);
        field.set_modulate(PLACEHOLDER);
        self.field_label = Some(field);
        // Hover- und Auswahl-Fläche einer Zeile
        let row_size = Vector2::new(WIDTH - 2.0 * PAD - BAR_W - BAR_MARGIN, ROW_H * 0.92);
        let hover = self.panel(row_size, Vector3::ZERO, Color::from_rgba(1.0, 1.0, 1.0, 0.0), 0.025, false, false);
        let select = self.panel(row_size, Vector3::ZERO, Color::from_rgba(0.95, 0.7, 0.25, 0.2), 0.025, false, false);
        select.1.clone().set_shader_parameter("border", &Color::from_rgba(1.0, 0.82, 0.45, 0.8).to_variant());
        select.1.clone().set_shader_parameter("border_w", &0.006f32.to_variant());
        self.hover_quad = Some(hover);
        self.select_quad = Some(select);
        // Scrollbalken: Bahn und Griff (Griffhöhe über die Skalierung eines Quads der Höhe 1)
        let track_h = h - 2.0 * PAD;
        self.track = Some(self.panel(Vector2::new(BAR_W, track_h), Vector3::new(bar_x(), 0.0, 0.003), Color::from_rgba(1.0, 1.0, 1.0, 0.06), BAR_W * 0.5, false, true));
        let thumb = self.panel(Vector2::new(BAR_W, 1.0), Vector3::new(bar_x(), 0.0, 0.006), Color::from_rgba(0.62, 0.62, 0.66, 0.75), BAR_W * 0.5, false, true);
        self.thumb = Some(thumb);
        // Zeilen
        for slot in 0..ROWS {
            let l = self.text_label(-WIDTH * 0.5 + PAD + 0.05, row_y(slot));
            self.labels.push(l);
        }
        // Trefferfläche über Filterfeld und Liste
        let mut area = Area3D::new_alloc();
        area.set_collision_layer(CONTROL_LAYER);
        area.set_collision_mask(0);
        area.set_monitoring(false);
        area.set_monitorable(false);
        let mut shape = BoxShape3D::new_gd();
        let total = h + FIELD_GAP + FIELD_H;
        shape.set_size(Vector3::new(WIDTH, total, 0.01));
        let mut col = CollisionShape3D::new_alloc();
        col.set_shape(&shape);
        area.add_child(&col);
        area.set_position(Vector3::new(0.0, (FIELD_GAP + FIELD_H) * 0.5, 0.01));
        self.base_mut().add_child(&area);
    }

    /// Abgerundete Fläche; `fades` = blendet beim Erscheinen mit ein (Hover/Auswahl steuern ihre
    /// Deckkraft selbst bzw. sind ausgeblendet).
    fn panel(&mut self, size: Vector2, pos: Vector3, fill: Color, radius: f32, icon: bool, fades: bool) -> (Gd<MeshInstance3D>, Gd<ShaderMaterial>) {
        let shader = crate::table::shared::shader(SHADER);
        let mut mat = ShaderMaterial::new_gd();
        mat.set_shader(&shader);
        mat.set_shader_parameter("size", &size.to_variant());
        mat.set_shader_parameter("radius", &radius.to_variant());
        mat.set_shader_parameter("fill", &fill.to_variant());
        mat.set_shader_parameter("icon", &(if icon { 1.0f32 } else { 0.0 }).to_variant());
        mat.set_shader_parameter("fade", &0.0f32.to_variant());
        mat.set_render_priority(1);
        let mut quad = QuadMesh::new_gd();
        quad.set_size(size);
        let mut mesh = MeshInstance3D::new_alloc();
        mesh.set_mesh(&quad);
        mesh.set_material_override(&mat);
        mesh.set_cast_shadows_setting(ShadowCastingSetting::OFF);
        mesh.set_position(pos);
        self.base_mut().add_child(&mesh);
        if fades {
            self.materials.push(mat.clone());
        } else {
            mat.set_shader_parameter("fade", &1.0f32.to_variant());
        }
        (mesh, mat)
    }

    fn text_label(&mut self, x: f32, y: f32) -> Gd<Label3D> {
        let mut l = Label3D::new_alloc();
        l.set_font_size(FONT);
        l.set_pixel_size(PIXEL);
        l.set_outline_size(6);
        l.set_modulate(TEXT);
        l.set_draw_flag(DrawFlags::SHADED, false);
        l.set_draw_flag(DrawFlags::DOUBLE_SIDED, false);
        l.set_horizontal_alignment(HorizontalAlignment::LEFT);
        l.set_render_priority(3);
        l.set_outline_render_priority(2);
        l.set_position(Vector3::new(x, y, 0.012));
        self.base_mut().add_child(&l);
        l
    }
}

/// Höhe der Liste (Zeilen und Innenabstand).
fn list_height() -> f32 {
    ROWS as f32 * ROW_H + 2.0 * PAD
}

/// Mitte der Zeile `slot` (0 = oben).
fn row_y(slot: usize) -> f32 {
    list_height() * 0.5 - PAD - (slot as f32 + 0.5) * ROW_H
}

/// Mitte des Scrollbalkens.
fn bar_x() -> f32 {
    WIDTH * 0.5 - BAR_MARGIN - BAR_W * 0.5
}

/// Zeichen je Zeile, bis zu denen ein Eintrag ganz in die Liste passt.
const MAX_CHARS: usize = 36;

/// Zu lange Einträge mit „…“ kürzen.
fn clip(text: &str) -> String {
    if text.chars().count() <= MAX_CHARS {
        text.to_string()
    } else {
        format!("{}…", text.chars().take(MAX_CHARS - 1).collect::<String>())
    }
}
