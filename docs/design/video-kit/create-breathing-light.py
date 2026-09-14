"""Generate the editable breathing-light study in a fresh background Blender process.

blender --background --factory-startup --python create-breathing-light.py -- --output breathing-light.blend
Add --render-dir frames to render the complete transparent PNG loop. Requires Blender 5.2.
"""

import argparse
import json
import sys
from pathlib import Path

import bpy

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("--output", type=Path, required=True)
parser.add_argument("--period", type=float, default=3.4)
parser.add_argument("--fps", type=int, default=30)
parser.add_argument("--render-dir", type=Path)
parser.add_argument("--force", action="store_true")
args = parser.parse_args(
    sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else []
)
if not bpy.app.background:
    raise RuntimeError(
        "Run this generator in a fresh background process, not an interactive Blender session"
    )
if not 1.5 <= args.period <= 8 or not 1 <= args.fps <= 60:
    raise ValueError("Use a period of 1.5–8 seconds and a frame rate of 1–60 fps")
args.output = args.output.resolve()
if args.output.exists() and not args.force:
    raise FileExistsError("Output exists; choose another path or pass --force")
args.output.parent.mkdir(parents=True, exist_ok=True)
bpy.ops.wm.read_factory_settings(use_empty=True)

scene = bpy.context.scene
scene.name = "Mluva / Breathing light"
scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = scene.render.resolution_y = 384
scene.render.resolution_percentage = 100
scene.render.fps = args.fps
scene.frame_start, scene.frame_end = 1, round(args.period * args.fps)
scene.render.film_transparent = True
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGBA"
scene.view_settings.view_transform = "Standard"
world = bpy.data.worlds.new("Soft studio")
world.use_nodes = True
world.node_tree.nodes["Background"].inputs["Color"].default_value = (0.5, 0.5, 0.5, 1)
world.node_tree.nodes["Background"].inputs["Strength"].default_value = 0.25
scene.world = world

controls = bpy.data.objects.new("Controls / Breath", None)
scene.collection.objects.link(controls)
controls["period_seconds"] = args.period
controls["amplitude"] = 0.12
controls["deformation"] = 0.16
controls["description"] = (
    "Procedural fluid silhouette. Change period, amplitude or deformation; all drivers update. Set the playback end to round(period_seconds * fps) after changing the period in the UI."
)
for key, low, high in [
    ("period_seconds", 1.5, 8),
    ("amplitude", 0, 0.3),
    ("deformation", 0, 0.4),
]:
    controls.id_properties_ui(key).update(min=low, max=high)

field = bpy.data.metaballs.new("Fluid / Continuous surface")
field.resolution = 0.045
field.render_resolution = 0.025
field.threshold = 0.6
blob = bpy.data.objects.new("Breathing light", field)
scene.collection.objects.link(blob)
base = field.elements.new()
base.radius = 1.7
left = field.elements.new()
left.co, left.radius = (-0.48, 0.12, 0), 0.88
right = field.elements.new()
right.co, right.radius = (0.48, -0.1, 0), 0.82


def driver(owner, path, expression, index=-1):
    """Connect one animated property to the shared breath controls and scene frame rate."""
    curve = owner.driver_add(path, index)
    curve.driver.type = "SCRIPTED"
    for name, prop in [
        ("period", "period_seconds"),
        ("amp", "amplitude"),
        ("deform", "deformation"),
    ]:
        variable = curve.driver.variables.new()
        variable.name, variable.type = name, "SINGLE_PROP"
        variable.targets[0].id = controls
        variable.targets[0].data_path = '["' + prop + '"]'
    variable = curve.driver.variables.new()
    variable.name, variable.type = "fps", "SINGLE_PROP"
    variable.targets[0].id_type = "SCENE"
    variable.targets[0].id = scene
    variable.targets[0].data_path = "render.fps"
    curve.driver.expression = expression.replace(
        "phase", "(2*pi*(frame-1)/(fps*period))"
    )


driver(blob, "scale", "1+amp*cos(phase)", 0)
driver(blob, "scale", "1+amp*cos(phase)", 1)
driver(blob, "scale", "0.72+0.06*cos(phase)", 2)
driver(left, "co", "-0.48+deform*cos(phase)", 0)
driver(left, "co", "0.12+deform*sin(phase)", 1)
driver(right, "co", "0.48+deform*sin(phase)", 0)
driver(right, "co", "-0.1+deform*cos(phase)", 1)
driver(right, "radius", "0.82+0.08*sin(phase)")
material = bpy.data.materials.new("Signal / Nord red")
material.use_nodes = True
shader = material.node_tree.nodes.get("Principled BSDF")


def linear(value):
    """Convert a display sRGB channel to Blender's scene-linear material color."""
    return value / 12.92 if value <= 0.04045 else ((value + 0.055) / 1.055) ** 2.4


shader.inputs["Base Color"].default_value = tuple(
    linear(value / 255) for value in (191, 97, 106)
) + (1,)
shader.inputs["Roughness"].default_value = 0.52
shader.inputs["Metallic"].default_value = 0
if "Subsurface Weight" in shader.inputs:
    shader.inputs["Subsurface Weight"].default_value = 0.1
field.materials.append(material)
camera_data = bpy.data.cameras.new("Camera / Transparent asset")
camera_data.type = "ORTHO"
camera_data.ortho_scale = 3.25
camera = bpy.data.objects.new("Camera / Transparent asset", camera_data)
camera.location = (0, 0, 6)
camera.rotation_euler = (0, 0, 0)
scene.collection.objects.link(camera)
scene.camera = camera
for name, position, power, size in [
    ("Key / broad softbox", (-3, -1, 5), 350, 5),
    ("Fill / soft edge", (3, 2, 3), 120, 4),
]:
    light_data = bpy.data.lights.new(name, "AREA")
    light_data.energy, light_data.shape, light_data.size = power, "DISK", size
    light = bpy.data.objects.new(name, light_data)
    scene.collection.objects.link(light)
    light.location = position
    light.rotation_euler = (-light.location).to_track_quat("-Z", "Y").to_euler()

scene["production_note"] = (
    "An art-directed metaball surface, not a physical liquid simulation. GTK and Quickshell use a light 2D harmonic silhouette with the same 3.4 s cadence."
)
scene.frame_set(1)
bpy.context.view_layer.update()
scene.render.filepath = "//frames/frame_"
bpy.ops.wm.save_as_mainfile(filepath=str(args.output))
if args.render_dir:
    args.render_dir = args.render_dir.resolve()
    args.render_dir.mkdir(parents=True, exist_ok=True)
    scene.render.filepath = str(args.render_dir / "frame_")
    bpy.ops.render.render(animation=True)
print(
    json.dumps(
        {
            "output": str(args.output),
            "period": args.period,
            "fps": args.fps,
            "frames": scene.frame_end,
            "transparent": scene.render.film_transparent,
        }
    )
)
