//! Exil-Strudel: dunkel wirbelnder Wirbel auf dem Tisch unter dem Exil-Stapel (nach dem Video
//! des Users „Exile and Grave“). Ein Shader auf einer flachen Fläche: Rauschen in Polarkoordinaten,
//! das sich langsam dreht und nach innen gezogen wird, außen weich ausgeblendet, in der Mitte ein
//! dunkles Loch. Karten im Exil verschwinden darin ([member Zone.swallow_cards]); ein Klick auf
//! den Strudel (Trefferfläche in der Gruppe [`ZONE_CLICK_GROUP`]) öffnet die Exil-Ansicht. Er ist nur offen, solange Karten im Exil liegen ([`set_exile_portal_open`]):
//! öffnet sich aus der Mitte (0,30 s, Back-Out) und schließt sich wieder (0,30 s, Quad-In).

use godot::classes::geometry_instance_3d::ShadowCastingSetting;
use godot::classes::tween::{EaseType, TransitionType};
use godot::classes::{Area3D, CylinderShape3D, CollisionShape3D, MeshInstance3D, QuadMesh, ShaderMaterial};
use godot::prelude::*;

use super::layers::CONTROL_LAYER;
use super::shader_noise::PERLIN_FBM;

/// Gruppe der Trefferflächen, deren Klick der
/// [`TableController`](super::table_controller::TableController) als `zone_clicked` meldet.
pub const ZONE_CLICK_GROUP: &str = "zone_click";

/// Durchmesser des Wirbels (Zonen-Einheiten) und Höhe über dem Tisch.
const PORTAL_SIZE: f32 = 1.45;
const PORTAL_LIFT: f32 = 0.006;
/// Öffnen / Schließen (Sekunden).
const PORTAL_TIME: f64 = 0.3;

const SHADER_HEAD: &str = r#"
shader_type spatial;
render_mode unshaded, blend_mix, cull_disabled, depth_draw_never, shadows_disabled;
uniform float open = 0.0;
"#;

const SHADER_BODY: &str = r#"
void fragment() {
    if (open <= 0.001) { discard; }
    vec2 p = (UV - 0.5) * 2.0 / open;
    float r = length(p);
    if (r > 1.0) { discard; }
    float a = atan(p.y, p.x);
    float t = TIME * 0.35;
    // Wirbel: Winkel dreht sich nach innen stärker, Muster wird zur Mitte gezogen
    float swirl = a + 2.6 / (r + 0.25) - t * 2.0;
    vec2 q = vec2(cos(swirl), sin(swirl)) * (1.2 + r * 1.8);
    float n = fbm(vec4(q * 1.4, r * 2.0 - t * 1.5, t * 0.4), 4, 0.55);
    float streak = smoothstep(0.52, 0.78, n);
    vec3 deep = vec3(0.015, 0.02, 0.035);
    vec3 mist = vec3(0.1, 0.13, 0.19);
    vec3 glint = vec3(0.42, 0.5, 0.66);
    vec3 col = mix(deep, mist, n * (0.4 + 0.6 * r));
    col = mix(col, glint, streak * 0.35 * smoothstep(0.15, 0.6, r));
    // dunkles Loch in der Mitte, weicher Rand außen
    col *= mix(0.25, 1.0, smoothstep(0.05, 0.45, r));
    float alpha = (1.0 - smoothstep(0.7, 1.0, r)) * mix(0.45, 0.8, smoothstep(0.1, 0.6, r)) * clamp(open, 0.0, 1.0);
    ALBEDO = col;
    ALPHA = alpha;
}
"#;

/// Legt den Exil-Strudel als Kind unter die Exil-Zone `zone` (flach auf dem Tisch, unter den
/// Karten). Mehrfaches Aufrufen legt nur einen an.
pub fn attach_exile_portal(zone: &mut Gd<Node3D>) {
    if zone.get_node_or_null("ExilePortal").is_some() {
        return;
    }
    let shader = crate::table::shared::shader(&format!("{SHADER_HEAD}{PERLIN_FBM}{SHADER_BODY}"));
    let mut mat = ShaderMaterial::new_gd();
    mat.set_shader(&shader);
    mat.set_shader_parameter("open", &0.0f32.to_variant());
    mat.set_render_priority(-2);
    let mut quad = QuadMesh::new_gd();
    quad.set_size(Vector2::splat(PORTAL_SIZE));
    quad.set_material(&mat);
    let mut mesh = MeshInstance3D::new_alloc();
    mesh.set_name("ExilePortal");
    mesh.set_mesh(&quad);
    mesh.set_cast_shadows_setting(ShadowCastingSetting::OFF);
    // Quad zeigt nach +Z → flach auf den Tisch legen (Normale nach oben)
    mesh.set_transform(Transform3D::new(
        Basis::from_axis_angle(Vector3::RIGHT, -std::f32::consts::FRAC_PI_2),
        Vector3::new(0.0, PORTAL_LIFT, 0.0),
    ));
    zone.add_child(&mesh);
    // Trefferfläche (nur bedienbar, solange der Strudel offen ist)
    let mut hit = Area3D::new_alloc();
    hit.set_name("ExilePortalHit");
    hit.set_collision_layer(0);
    hit.set_collision_mask(0);
    hit.set_monitoring(false);
    hit.set_monitorable(false);
    hit.add_to_group(ZONE_CLICK_GROUP);
    let mut shape = CylinderShape3D::new_gd();
    shape.set_radius(PORTAL_SIZE * 0.5);
    shape.set_height(0.1);
    let mut col = CollisionShape3D::new_alloc();
    col.set_shape(&shape);
    hit.add_child(&col);
    hit.set_position(Vector3::new(0.0, 0.05, 0.0));
    zone.add_child(&hit);
}

/// Exil-Strudel unter `zone` öffnen (`open` = Karten im Exil) bzw. schließen; animiert, nur
/// wenn sich der Zustand ändert.
pub fn set_exile_portal_open(zone: &mut Gd<Node3D>, open: bool) {
    let Some(mut mesh) = zone.get_node_or_null("ExilePortal").and_then(|n| n.try_cast::<MeshInstance3D>().ok()) else { return };
    if mesh.has_meta("portal_open") && mesh.get_meta("portal_open").try_to::<bool>().ok() == Some(open) {
        return;
    }
    mesh.set_meta("portal_open", &open.to_variant());
    if let Some(mut hit) = zone.get_node_or_null("ExilePortalHit").and_then(|n| n.try_cast::<Area3D>().ok()) {
        hit.set_collision_layer(if open { CONTROL_LAYER } else { 0 });
    }
    let Some(mat) = mesh.get_mesh().and_then(|m| m.surface_get_material(0)).and_then(|m| m.try_cast::<ShaderMaterial>().ok()) else { return };
    let from = mat.get_shader_parameter("open").try_to::<f32>().unwrap_or(0.0);
    let to = if open { 1.0f32 } else { 0.0 };
    // Wert über eine Funktion setzen: der Shader-Parameter ist vor dem ersten Kompilieren noch
    // keine Eigenschaft, die ein Tween direkt ansprechen könnte
    let mut target = mat.clone();
    let set = Callable::from_linked_fn("set_portal_open", &mat, move |args: &[&Variant]| {
        let v = args.first().and_then(|a| a.try_to::<f32>().ok()).unwrap_or(0.0);
        target.set_shader_parameter("open", &v.to_variant());
    });
    let mut tween = mesh.create_tween();
    tween.set_trans(if open { TransitionType::BACK } else { TransitionType::QUAD });
    tween.set_ease(if open { EaseType::OUT } else { EaseType::IN });
    tween.tween_method(&set, &from.to_variant(), &to.to_variant(), PORTAL_TIME);
}
