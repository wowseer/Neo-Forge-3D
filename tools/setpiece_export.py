"""Spielfeld aus Blender (Set Pieces/*_Spielfeld.blend) für den Tisch als .glb exportieren.

Aufruf (ohne Fenster):
    blender -b "Set Pieces/Ebene_Spielfeld.blend" --python tools/setpiece_export.py -- godot/setpieces/ebene.glb

Die Spielfelder nutzen prozedurale Blender-Materialien (Noise, Voronoi, Bump …), die glTF und
Godot nicht kennen. Das Skript backt sie deshalb in Texturen:

1. Kameras und nicht gerenderte Objekte entfernen, Flächenlichter zu Spots, Instanzen auflösen,
   Modifikatoren anwenden.
2. Objekte mit mehreren Materialien nach Material trennen (Ursprung bleibt, damit
   Objekt-Koordinaten der Materialien gleich bleiben).
3. Je Material: alle Objekte gemeinsam abwickeln (Smart UV, eine gemeinsame Textur) und Farbe
   (Diffuse Color, mit der Umgebungsverdeckung der Materialien) sowie – wo das Material Bump hat –
   die Normalen backen. Größe der Textur nach Fläche (TEX_PER_M), 128 … 2048 px.
4. Material durch ein einfaches ersetzen (Bild → Basisfarbe, Normal-Map, Rauheit/Metall/Emission
   wie eingestellt) und erst dann je Material alle Objekte zu einem verbinden (wenige
   Zeichenaufrufe statt hunderter Objekte).
5. Als GLB mit Punktlichtern exportieren (gebackene Bilder als PNG eingebettet).

Maßstab 1:1 (1 m in Blender = 1 Einheit im Tisch), Spielfläche mit Oberkante bei z = 0.
"""
import math
import os
import sys
import time

import bpy

TEX_PER_M = 64
TEX_MIN = 128
TEX_MAX = 1024
# Spielfläche: schärfer (darauf liegen die Karten)
TEX_MAX_BOARD = 2048
SAMPLES = 4

out = sys.argv[sys.argv.index("--") + 1]
# gebackene Bilder (Zwischendateien, nach dem Export gelöscht)
import tempfile
TMP_DIR = tempfile.mkdtemp(prefix="setpiece-")
t0 = time.time()
scene = bpy.context.scene
view_layer = bpy.context.view_layer


def log(*a):
    print("[setpiece]", f"{time.time() - t0:6.1f}s", *a, flush=True)


def select_only(objs, active=None):
    bpy.ops.object.select_all(action="DESELECT")
    for o in objs:
        o.select_set(True)
    view_layer.objects.active = active or (objs[0] if objs else None)


# --- 1. aufräumen --------------------------------------------------------------------------------
if bpy.context.object and bpy.context.object.mode != "OBJECT":
    bpy.ops.object.mode_set(mode="OBJECT")
for coll in bpy.data.collections:
    coll.hide_viewport = False
for o in list(scene.objects):
    o.hide_viewport = False
    o.hide_set(False)
    if o.type == "CAMERA" or o.hide_render:
        bpy.data.objects.remove(o, do_unlink=True)
# Flächenlichter (z. B. „Hauptlicht“ über dem Tisch) kennt glTF nicht: als breiten Spot exportieren
for o in scene.objects:
    if o.type == "LIGHT" and o.data.type == "AREA":
        o.data.type = "SPOT"
        o.data.spot_size = math.radians(150)
        o.data.spot_blend = 0.6
select_only([o for o in scene.objects if o.instance_type == "COLLECTION"])
if bpy.context.selected_objects:
    bpy.ops.object.duplicates_make_real()
geo = [o for o in scene.objects if o.type in {"MESH", "CURVE", "SURFACE", "META", "FONT"}]
select_only(geo)
bpy.ops.object.make_single_user(object=True, obdata=True, material=False)
bpy.ops.object.convert(target="MESH")
meshes = [o for o in scene.objects if o.type == "MESH" and len(o.data.polygons) > 0]
log("Objekte:", len(meshes))

# --- 2. nach Material trennen --------------------------------------------------------------------
multi = [o for o in meshes if len({p.material_index for p in o.data.polygons}) > 1]
for o in multi:
    select_only([o])
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.separate(type="MATERIAL")
    bpy.ops.object.mode_set(mode="OBJECT")
meshes = [o for o in scene.objects if o.type == "MESH" and len(o.data.polygons) > 0]


def main_material(o):
    if not o.data.polygons or not o.material_slots:
        return None
    idx = o.data.polygons[0].material_index
    return o.material_slots[idx].material if idx < len(o.material_slots) else None


groups = {}
for o in meshes:
    m = main_material(o)
    # nur das eine Material behalten (nach dem Trennen hat jedes Objekt Flächen eines Materials)
    if m is not None:
        o.data.materials.clear()
        o.data.materials.append(m)
    groups.setdefault(m.name if m else "", []).append(o)
log("Materialien:", len(groups))


# --- 3. backen -----------------------------------------------------------------------------------
def principled(mat):
    if not mat or not mat.use_nodes:
        return None
    for n in mat.node_tree.nodes:
        if n.type == "BSDF_PRINCIPLED":
            return n
    return None


def linked(node, name):
    s = node.inputs.get(name)
    return s is not None and s.is_linked


def world_area(objs):
    total = 0.0
    for o in objs:
        sx, sy, sz = (abs(v) for v in o.matrix_world.to_scale())
        k = (sx * sy + sy * sz + sx * sz) / 3.0
        total += sum(p.area for p in o.data.polygons) * k
    return total


scene.render.engine = "CYCLES"
scene.cycles.device = "CPU"
scene.cycles.samples = SAMPLES
scene.render.bake.margin = 4
scene.render.bake.use_clear = True

for name, objs in groups.items():
    mat = bpy.data.materials.get(name)
    bsdf = principled(mat)
    if bsdf is None:
        continue
    bake_color = linked(bsdf, "Base Color")
    bake_normal = linked(bsdf, "Normal")
    if not (bake_color or bake_normal):
        continue
    area = world_area(objs)
    board = any(o.name.startswith("Spielflaeche") for o in objs)
    size = int(min(TEX_MAX_BOARD if board else TEX_MAX,
                   max(TEX_MIN, 2 ** math.ceil(math.log2(max(1.0, math.sqrt(area) * (TEX_PER_M * 2 if board else TEX_PER_M)))))))
    log(f"backe {name}: {len(objs)} Obj., {size}px …")
    # gemeinsame Abwicklung aller Objekte dieses Materials
    select_only(objs)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.uv.smart_project(angle_limit=math.radians(66), island_margin=0.004, scale_to_bounds=False)
    bpy.ops.object.mode_set(mode="OBJECT")
    nodes = mat.node_tree.nodes
    results = {}
    for kind, wanted in (("DIFFUSE", bake_color), ("NORMAL", bake_normal)):
        if not wanted:
            continue
        img = bpy.data.images.new(f"{name}_{kind.lower()}", size, size, alpha=False)
        if kind == "NORMAL":
            img.colorspace_settings.name = "Non-Color"
        tex = nodes.new("ShaderNodeTexImage")
        tex.image = img
        for n in nodes:
            n.select = False
        tex.select = True
        nodes.active = tex
        select_only(objs)
        if kind == "DIFFUSE":
            bpy.ops.object.bake(type="DIFFUSE", pass_filter={"COLOR"}, use_selected_to_active=False)
        else:
            bpy.ops.object.bake(type="NORMAL", normal_space="TANGENT", use_selected_to_active=False)
        # als PNG-Datei speichern und von dort neu laden: der Export bettet sie dann unverändert
        # ein (Blenders JPEG-Dateien konnte Godots Bildlader nicht lesen; Godot komprimiert die
        # Texturen beim Import ohnehin selbst)
        img.filepath_raw = os.path.join(TMP_DIR, f"{name}_{kind.lower()}.png")
        img.file_format = "PNG"
        img.save()
        img.source = "FILE"
        img.reload()
        nodes.remove(tex)
        results[kind] = img
    log(f"gebacken: {name} ({len(objs)} Obj., {size}px, {'+'.join(results)})")

    # --- 4. einfaches Material ---
    rough = bsdf.inputs["Roughness"].default_value if not linked(bsdf, "Roughness") else 0.75
    metal = bsdf.inputs["Metallic"].default_value if not linked(bsdf, "Metallic") else 0.0
    base = tuple(bsdf.inputs["Base Color"].default_value)
    emis_in = bsdf.inputs.get("Emission Color")
    emis = tuple(emis_in.default_value) if emis_in is not None and not emis_in.is_linked else (0, 0, 0, 1)
    emis_str = bsdf.inputs["Emission Strength"].default_value if "Emission Strength" in bsdf.inputs else 0.0
    alpha = bsdf.inputs["Alpha"].default_value if not linked(bsdf, "Alpha") else 1.0
    nodes.clear()
    outn = nodes.new("ShaderNodeOutputMaterial")
    p = nodes.new("ShaderNodeBsdfPrincipled")
    links = mat.node_tree.links
    links.new(p.outputs["BSDF"], outn.inputs["Surface"])
    p.inputs["Roughness"].default_value = rough
    p.inputs["Metallic"].default_value = metal
    p.inputs["Alpha"].default_value = alpha
    p.inputs["Emission Color"].default_value = emis
    p.inputs["Emission Strength"].default_value = emis_str
    if "DIFFUSE" in results:
        t = nodes.new("ShaderNodeTexImage")
        t.image = results["DIFFUSE"]
        links.new(t.outputs["Color"], p.inputs["Base Color"])
    else:
        p.inputs["Base Color"].default_value = base
    if "NORMAL" in results:
        t = nodes.new("ShaderNodeTexImage")
        t.image = results["NORMAL"]
        nm = nodes.new("ShaderNodeNormalMap")
        links.new(t.outputs["Color"], nm.inputs["Color"])
        links.new(nm.outputs["Normal"], p.inputs["Normal"])

# --- 4b. je Material verbinden -------------------------------------------------------------------
joined = 0
for name, objs in groups.items():
    objs = [o for o in objs if o.name in scene.objects]
    if len(objs) > 1:
        select_only(objs)
        bpy.ops.object.join()
    if objs:
        view_layer.objects.active.name = f"Setpiece_{name or 'ohne'}"
        joined += 1
log("Objekte nach dem Verbinden:", joined)

# --- 5. export -----------------------------------------------------------------------------------
os.makedirs(os.path.dirname(os.path.abspath(out)), exist_ok=True)
bpy.ops.object.select_all(action="DESELECT")
bpy.ops.export_scene.gltf(
    filepath=out,
    export_format="GLB",
    export_lights=True,
    export_cameras=False,
    export_apply=True,
    export_yup=True,
    export_image_format="AUTO",
)
import shutil
shutil.rmtree(TMP_DIR, ignore_errors=True)
log("fertig:", out, f"{os.path.getsize(out) / 1e6:.1f} MB")
