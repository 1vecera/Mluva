"""Build an original six-second loop of lit, gently moving obsidian waves in Blender."""

import math
from pathlib import Path

import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parents[2]
# Run in a disposable Blender process with --factory-startup. The build owns
# that fresh scene only; it never connects to a user's existing .blend file.
scene = bpy.context.scene
scene.name = "Mluva Obsidian"
for obj in list(scene.objects):
    bpy.data.objects.remove(obj, do_unlink=True)
scene.render.engine = "BLENDER_EEVEE"
scene.render.resolution_x = 1920
scene.render.resolution_y = 1080
scene.render.resolution_percentage = 100
scene.eevee.taa_render_samples = 32
scene.render.fps = 30
scene.frame_start, scene.frame_end = 1, 180
scene.render.image_settings.file_format = "PNG"
scene.render.image_settings.color_mode = "RGB"
output = ROOT / "tmp/background"
output.mkdir(parents=True, exist_ok=True)
scene.render.filepath = str(output / "frame-")
scene.world = bpy.data.worlds.new("Mluva Dark Studio")
scene.world.use_nodes = True
scene.world.node_tree.nodes["Background"].inputs[0].default_value = (
    0.016,
    0.022,
    0.035,
    1,
)
scene.world.node_tree.nodes["Background"].inputs[1].default_value = 0.4
scene.view_settings.view_transform = "AgX"

vertices, faces = [], []
nx, ny = 160, 110
for row in range(ny + 1):
    y = -13 + 26 * row / ny
    for column in range(nx + 1):
        x = -20 + 40 * column / nx
        z = 0.75 * math.sin(x * 0.23 + y * 0.44) + 0.22 * math.sin(x * 0.62 - y * 0.17)
        vertices.append((x, y, z))
        if row < ny and column < nx:
            a = row * (nx + 1) + column
            faces.append((a, a + 1, a + nx + 2, a + nx + 1))
mesh = bpy.data.meshes.new("Continuous wave surface")
mesh.from_pydata(vertices, [], faces)
mesh.update()
surface = bpy.data.objects.new("Obsidian silk", mesh)
scene.collection.objects.link(surface)
for polygon in mesh.polygons:
    polygon.use_smooth = True
surface.shape_key_add(name="Basis")
for axis, trig in [("Sine", "sin"), ("Cosine", "cos")]:
    key = surface.shape_key_add(name=axis)
    key.slider_min = -1
    for index, vertex in enumerate(vertices):
        x, y, z = vertex
        offset = 0.16 * (math.sin if axis == "Sine" else math.cos)(x * 0.23 + y * 0.44)
        key.data[index].co.z = z + offset
    curve = key.driver_add("value")
    curve.driver.expression = f"{trig}((frame-1)*2*pi/180)"
material = bpy.data.materials.new("Deep blue polished stone")
material.use_nodes = True
principled = material.node_tree.nodes.get("Principled BSDF")
principled.inputs["Base Color"].default_value = (0.012, 0.020, 0.030, 1)
principled.inputs["Metallic"].default_value = 0.76
principled.inputs["Roughness"].default_value = 0.13
principled.inputs["Coat Weight"].default_value = 0.38
principled.inputs["Coat Roughness"].default_value = 0.1
surface.data.materials.append(material)


def area(name, position, color, energy, size, target):
    light = bpy.data.lights.new(name, "AREA")
    light.energy, light.color, light.shape, light.size = (
        energy,
        color,
        "RECTANGLE",
        size,
    )
    light.size_y = 0.5
    obj = bpy.data.objects.new(name, light)
    scene.collection.objects.link(obj)
    obj.location = position
    obj.rotation_euler = (
        (Vector(target) - obj.location).to_track_quat("-Z", "Y").to_euler()
    )


area("Frost softbox", (0, 3, 10), (0.49, 0.74, 1.0), 1700, 18, (0, 0, 0))
area("Crimson reflection", (-9, -3, 8), (1.0, 0.035, 0.065), 1400, 13, (-3, 0, 0))
area("Silver edge", (10, 1, 6), (0.78, 0.88, 1), 1900, 18, (4, 1, 0))
camera_data = bpy.data.cameras.new("Wide product backdrop")
camera = bpy.data.objects.new("Wide product backdrop", camera_data)
scene.collection.objects.link(camera)
camera.location = (0, -7, 16)
camera.rotation_euler = (
    (Vector((0, 0, 0)) - camera.location).to_track_quat("-Z", "Y").to_euler()
)
camera_data.type = "ORTHO"
camera_data.ortho_scale = 27
scene.camera = camera
scene.frame_set(1)
scene.render.film_transparent = False
path = output / "obsidian.blend"
bpy.ops.wm.save_as_mainfile(filepath=str(path), copy=True)
result = {
    "path": str(path),
    "scene": scene.name,
    "objects": len(scene.objects),
    "frames": 180,
    "fps": 30,
}
