//! TriBatch: gefüllte Flächen sammeln und mit einem einzigen Zeichenbefehl ausgeben.
//!
//! Godot legt für jedes `draw_polygon`, `draw_circle` und `draw_ellipse` eigene Puffer auf der
//! Grafikkarte an und gibt sie beim nächsten Neuzeichnen wieder frei. Anzeigen, die jedes Bild neu
//! gezeichnet werden und aus vielen Flächen bestehen (Medaillons: Ring aus 72 Segmenten,
//! Strahlenkranz), kosteten so mehrere Millisekunden je Bild. `TriBatch` sammelt die Flächen in
//! Zeichenreihenfolge (gleiche Zerlegung wie Godot) und gibt sie als ein Dreiecksfeld aus – das
//! Bild bleibt gleich. Vor jedem anderen Zeichenbefehl (Linien, Bögen, Text, Texturen)
//! [`TriBatch::flush`] aufrufen, damit die Reihenfolge stimmt.

use std::f32::consts::TAU;

use godot::classes::{Geometry2D, RenderingServer};
use godot::prelude::*;

/// Segmente eines Kreises bzw. einer Ellipse (wie Godots `draw_circle`/`draw_ellipse`).
const SEGMENTS: usize = 64;

/// Gesammelte Dreiecke für ein `CanvasItem` (siehe Moduldoku).
pub struct TriBatch {
    item: Rid,
    points: Vec<Vector2>,
    colors: Vec<Color>,
    indices: Vec<i32>,
}

impl TriBatch {
    /// Sammler für das `CanvasItem` mit der RID `item` (`get_canvas_item()`, nur in `draw()`).
    pub fn new(item: Rid) -> Self {
        Self { item, points: Vec::new(), colors: Vec::new(), indices: Vec::new() }
    }

    /// Wie `draw_polygon`: einfaches Polygon mit einer Farbe je Punkt (oder einer für alle).
    pub fn polygon(&mut self, pts: &[Vector2], cols: &[Color]) {
        let tri = Geometry2D::singleton().triangulate_polygon(&PackedVector2Array::from(pts));
        if tri.is_empty() {
            return;
        }
        let base = self.points.len() as i32;
        self.points.extend_from_slice(pts);
        if cols.len() == pts.len() {
            self.colors.extend_from_slice(cols);
        } else {
            let c = cols.first().copied().unwrap_or(Color::WHITE);
            self.colors.extend(std::iter::repeat_n(c, pts.len()));
        }
        self.indices.extend(tri.as_slice().iter().map(|i| base + i));
    }

    /// Wie `draw_colored_polygon`.
    pub fn colored_polygon(&mut self, pts: &[Vector2], color: Color) {
        self.polygon(pts, &[color]);
    }

    /// Wie `draw_circle` (gefüllt, ohne Kantenglättung).
    pub fn circle(&mut self, c: Vector2, r: f32, color: Color) {
        self.ellipse(c, r, r, color);
    }

    /// Wie `draw_ellipse` (gefüllt, ohne Kantenglättung): Halbachsen `rx` (waagrecht), `ry`.
    pub fn ellipse(&mut self, c: Vector2, rx: f32, ry: f32, color: Color) {
        let base = self.points.len() as i32;
        for i in 0..SEGMENTS {
            let a = i as f32 * TAU / SEGMENTS as f32;
            self.points.push(c + Vector2::new(a.cos() * rx, a.sin() * ry));
            self.colors.push(color);
        }
        // Fächer ab dem ersten Randpunkt (wie Godot)
        for i in 0..SEGMENTS as i32 - 2 {
            self.indices.extend([base, base + i + 1, base + i + 2]);
        }
    }

    /// Gesammelte Flächen als ein Dreiecksfeld zeichnen.
    pub fn flush(&mut self) {
        if self.indices.is_empty() {
            return;
        }
        RenderingServer::singleton().canvas_item_add_triangle_array(
            self.item,
            &PackedInt32Array::from(self.indices.as_slice()),
            &PackedVector2Array::from(self.points.as_slice()),
            &PackedColorArray::from(self.colors.as_slice()),
        );
        self.points.clear();
        self.colors.clear();
        self.indices.clear();
    }
}

impl Drop for TriBatch {
    /// Vergessene Reste noch zeichnen.
    fn drop(&mut self) {
        self.flush();
    }
}
