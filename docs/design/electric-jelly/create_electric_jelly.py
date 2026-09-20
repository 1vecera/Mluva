"""Build the Electric Jelly logo from the selected Figma contour in Blender.

Run with Blender's --background --python option, or execute through Blender MCP.
The original scene is preserved. A new scene contains editable geometry and lights.
"""

import argparse
import math
import re
import sys
import xml.etree.ElementTree as ET
from pathlib import Path

import bmesh
import bpy
from mathutils import Vector

ROOT = Path(__file__).resolve().parent
sys.path.insert(0, str(ROOT))
from reflection_cards import create_reflectors


def linear(hex_color):
    """Convert an sRGB design swatch to Blender's linear working space."""
    rgb = [int(hex_color[i : i + 2], 16) / 255 for i in (0, 2, 4)]
    return tuple(c / 12.92 if c <= 0.04045 else ((c + 0.055) / 1.055) ** 2.4 for c in rgb)


def read_contour(path):
    """Subdivide the exported polygon smoothly without replacing its silhouette."""
    root = ET.parse(path).getroot()
    data = root.find("{http://www.w3.org/2000/svg}path").attrib["d"]
    values = [float(n) for n in re.findall(r"-?\d+(?:\.\d+)?", data)]
    points = [Vector(((values[i] - 140) / 100, (140 - values[i + 1]) / 100)) for i in range(0, len(values), 2)]
    if (points[0] - points[-1]).length < 0.001:
        points.pop()
    contour = []
    for i, p1 in enumerate(points):
        p0, p2, p3 = (
            points[i - 1],
            points[(i + 1) % len(points)],
            points[(i + 2) % len(points)],
        )
        for step in range(4):
            t = step / 4
            contour.append(
                0.5
                * (
                    (2 * p1)
                    + (-p0 + p2) * t
                    + (2 * p0 - 5 * p1 + 4 * p2 - p3) * t * t
                    + (-p0 + 3 * p1 - 3 * p2 + p3) * t * t * t
                )
            )
    return contour


def make_volume(contour, collection):
    """Inflate the contour into a closed, smooth lens with a soft rounded edge."""
    count = len(contour)
    rings = 80
    thickness = 0.48
    vertices = [(0, 0, thickness)]
    for ring in range(1, rings):
        theta = math.pi * ring / rings
        radius = math.sin(theta)
        depth = thickness * math.cos(theta)
        for point in contour:
            vertices.append((point.x * radius, point.y * radius, depth))
    vertices.append((0, 0, -thickness))
    faces = []
    for i in range(count):
        faces.append((0, 1 + i, 1 + (i + 1) % count))
    for ring in range(rings - 2):
        start = 1 + ring * count
        for i in range(count):
            nxt = (i + 1) % count
            faces.append((start + i, start + count + i, start + count + nxt, start + nxt))
    end = len(vertices) - 1
    start = 1 + (rings - 2) * count
    for i in range(count):
        faces.append((end, start + (i + 1) % count, start + i))
    mesh = bpy.data.meshes.new("Electric Jelly / inflated Figma contour")
    mesh.from_pydata(vertices, [], faces)
    bm = bmesh.new()
    bm.from_mesh(mesh)
    bmesh.ops.recalc_face_normals(bm, faces=bm.faces)
    bm.to_mesh(mesh)
    bm.free()
    mesh.update()
    for polygon in mesh.polygons:
        polygon.use_smooth = True
    obj = bpy.data.objects.new("Electric Jelly / volume", mesh)
    collection.objects.link(obj)
    smooth = obj.modifiers.new("Surface refinement", "SUBSURF")
    smooth.levels = 1
    smooth.render_levels = 1
    obj["source"] = "Figma 206:1683, selected study 206:1681"
    obj["material_reference"] = "Figma 203:1673"
    return obj


def make_material():
    """Layer clear wet reflections over a saturated red translucent body."""
    material = bpy.data.materials.new("Electric Jelly / red gel")
    material.diffuse_color = (0.94, 0.0006, 0.003, 1)
    material.use_nodes = True
    nodes = material.node_tree.nodes
    shader = nodes.get("Principled BSDF")
    shader.label = "Red gel with clear wet surface"
    shader.inputs["Base Color"].default_value = (0.94, 0.0006, 0.003, 1)
    shader.inputs["Roughness"].default_value = 0.136
    shader.inputs["IOR"].default_value = 1.42
    shader.inputs["Transmission Weight"].default_value = 0.6864
    shader.inputs["Subsurface Weight"].default_value = 0.4025
    shader.inputs["Subsurface Radius"].default_value = (1, 0.12, 0.08)
    shader.inputs["Subsurface Scale"].default_value = 0.4025
    shader.inputs["Coat Weight"].default_value = 0.5
    shader.inputs["Coat Roughness"].default_value = 0.14
    shader.inputs["Coat IOR"].default_value = 1.46
    shader.inputs["Emission Color"].default_value = (1, 0.18, 0.10, 1)
    shader.location = (380, 100)
    output = nodes.get("Material Output")
    output.location = (740, 100)
    absorption = nodes.new("ShaderNodeVolumeAbsorption")
    absorption.label = "Red absorption through thickness"
    absorption.inputs["Color"].default_value = (0.9, 0.008, 0.012, 1)
    absorption.inputs["Density"].default_value = 0.09
    absorption.location = (380, -420)
    links = material.node_tree.links
    links.new(absorption.outputs[0], output.inputs["Volume"])
    geometry = nodes.new("ShaderNodeNewGeometry")
    geometry.location = (-820, 140)
    dot = nodes.new("ShaderNodeVectorMath")
    dot.operation = "DOT_PRODUCT"
    dot.location = (-620, 140)
    links.new(geometry.outputs["Normal"], dot.inputs[0])
    links.new(geometry.outputs["Incoming"], dot.inputs[1])
    absolute = nodes.new("ShaderNodeMath")
    absolute.operation = "ABSOLUTE"
    absolute.location = (-430, 140)
    links.new(dot.outputs["Value"], absolute.inputs[0])
    edge = nodes.new("ShaderNodeMath")
    edge.operation = "SUBTRACT"
    edge.inputs[0].default_value = 1
    edge.location = (-250, 140)
    links.new(absolute.outputs[0], edge.inputs[1])
    power = nodes.new("ShaderNodeMath")
    power.operation = "POWER"
    power.inputs[1].default_value = 2.2
    power.location = (-70, 140)
    links.new(edge.outputs[0], power.inputs[0])
    glow = nodes.new("ShaderNodeMath")
    glow.operation = "MULTIPLY_ADD"
    glow.label = "Thin coral edge glow"
    glow.inputs[1].default_value = 1.6
    glow.inputs[2].default_value = 0.012
    glow.location = (110, 140)
    links.new(power.outputs[0], glow.inputs[0])
    links.new(glow.outputs[0], shader.inputs["Emission Strength"])
    return material


def add_light(collection, name, position, energy, size, size_y, color="FFFFFF", target=(0, 0, 0)):
    """Aim a studio softbox so its reflection wraps around the gel surface."""
    data = bpy.data.lights.new(name, "AREA")
    data.energy = energy
    data.shape = "ELLIPSE"
    data.size = size
    data.size_y = size_y
    data.color = linear(color)
    obj = bpy.data.objects.new(name, data)
    collection.objects.link(obj)
    obj.location = position
    obj.rotation_euler = (Vector(target) - obj.location).to_track_quat("-Z", "Y").to_euler()
    return obj


def build(output_path, size, samples):
    """Create a standalone art scene while leaving existing scenes intact."""
    scene = bpy.data.scenes.new("Mluva / Electric Jelly")
    bpy.context.window.scene = scene
    body_collection = bpy.data.collections.new("Logo geometry")
    studio = bpy.data.collections.new("Studio reflections")
    scene.collection.children.link(body_collection)
    scene.collection.children.link(studio)
    logo = make_volume(read_contour(ROOT / "source/figma-contour.svg"), body_collection)
    logo.data.materials.append(make_material())

    camera_data = bpy.data.cameras.new("Electric Jelly / orthographic camera")
    camera = bpy.data.objects.new("Camera / front", camera_data)
    studio.objects.link(camera)
    camera.location = (0, 0, 8)
    camera_data.type = "ORTHO"
    camera_data.ortho_scale = 4.32
    scene.camera = camera
    backlight = add_light(studio, "Glow / transmitted backlight", (-0.3, -1.1, -2.6), 40, 3.4, 3.4)
    backlight.data.shape = "DISK"
    cards = bpy.data.collections.new("Shaped reflection cards")
    scene.collection.children.link(cards)
    create_reflectors(logo, cards, ROOT / "source/reflection-cards.json")

    world = bpy.data.worlds.new("Electric blue / neutral studio illumination")
    scene.world = world
    world.use_nodes = True
    nodes = world.node_tree.nodes
    nodes.clear()
    out = nodes.new("ShaderNodeOutputWorld")
    out.location = (500, 0)
    mix = nodes.new("ShaderNodeMixShader")
    mix.location = (260, 0)
    rays = nodes.new("ShaderNodeLightPath")
    rays.location = (-230, 200)
    neutral = nodes.new("ShaderNodeBackground")
    neutral.label = "Neutral lighting and refraction"
    neutral.inputs["Color"].default_value = (1, 1, 1, 1)
    neutral.inputs["Strength"].default_value = 0.036
    neutral.location = (-200, -40)
    blue = nodes.new("ShaderNodeBackground")
    blue.label = "Electric blue visible backdrop"
    blue.inputs["Color"].default_value = (*linear("1111FF"), 1)
    blue.inputs["Strength"].default_value = 1
    blue.location = (-20, -180)
    links = world.node_tree.links
    warm = nodes.new("ShaderNodeBackground")
    warm.label = "Warm reflection surround"
    warm.inputs["Color"].default_value = (1, 0.024, 0.008, 1)
    warm.inputs["Strength"].default_value = 0.5
    warm.location = (-200, -200)
    reflection = nodes.new("ShaderNodeMixShader")
    reflection.label = "Reflection-only warm surround"
    reflection.location = (20, -40)
    links.new(rays.outputs["Is Reflection Ray"], reflection.inputs[0])
    links.new(neutral.outputs[0], reflection.inputs[1])
    links.new(warm.outputs[0], reflection.inputs[2])
    links.new(rays.outputs["Is Camera Ray"], mix.inputs[0])
    links.new(reflection.outputs[0], mix.inputs[1])
    links.new(blue.outputs[0], mix.inputs[2])
    links.new(mix.outputs[0], out.inputs[0])

    scene.render.engine = "CYCLES"
    scene.cycles.samples = samples
    scene.cycles.use_denoising = True
    scene.cycles.max_bounces = 12
    scene.cycles.transmission_bounces = 8
    preferences = bpy.context.preferences.addons["cycles"].preferences
    preferences.get_devices()
    if any(device.type == "METAL" for device in preferences.devices):
        preferences.compute_device_type = "METAL"
        for device in preferences.devices:
            device.use = device.type == "METAL"
        scene.cycles.device = "GPU"
    scene.render.resolution_x = size
    scene.render.resolution_y = size
    scene.render.resolution_percentage = 100
    scene.render.image_settings.file_format = "PNG"
    scene.render.image_settings.color_mode = "RGBA"
    scene.view_settings.view_transform = "Standard"
    scene.view_settings.look = "None"
    scene.view_settings.exposure = 0
    scene.view_settings.gamma = 1
    scene.render.filepath = "//renders/electric-jelly.png"
    scene["reference"] = "https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=203-1673"
    scene["selected_vector"] = "https://www.figma.com/design/4mdtod74gknaCCm8q1nrDj?node-id=206-1681"
    bpy.context.view_layer.objects.active = logo
    logo.select_set(True)
    for screen in bpy.data.screens:
        for area in screen.areas:
            if area.type == "VIEW_3D":
                area.spaces.active.region_3d.view_perspective = "CAMERA"
                area.spaces.active.overlay.show_overlays = False
    source = bpy.data.texts.new("README / Electric Jelly")
    source.use_fake_user = True
    source.write(
        "Hero view from Figma 206:1681, material reference 203:1673.\n"
        "Logo geometry is a closed mesh with a non-destructive subdivision modifier.\n"
        "Shaped reflection cards are real studio meshes, visible to glossy rays only.\n"
        "Adjust each card material Emission Strength to rebalance a highlight.\n"
        "The camera is orthographic. Cards are tuned for this front view; re-aim for other views.\n"
        "Set Render > Film > Transparent for the isolated mark.\n"
    )
    bpy.ops.wm.save_as_mainfile(filepath=str(output_path), compress=True)
    return scene


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, default=ROOT / "electric-jelly.blend")
    parser.add_argument("--size", type=int, default=1200)
    parser.add_argument("--samples", type=int, default=128)
    parser.add_argument("--render", action="store_true")
    parser.add_argument("--force", action="store_true")
    args = parser.parse_args(sys.argv[sys.argv.index("--") + 1 :] if "--" in sys.argv else [])
    if not bpy.app.background:
        raise RuntimeError("Run the generator in a separate background Blender process.")
    if args.output.exists() and not args.force:
        raise FileExistsError("Output exists; choose a new path or explicitly pass --force.")
    if args.size < 64 or args.samples < 1:
        raise ValueError("Use at least 64 pixels and one sample.")
    args.output = args.output.resolve()
    args.output.parent.mkdir(parents=True, exist_ok=True)
    scene = build(args.output, args.size, args.samples)
    if args.render:
        render_dir = args.output.parent / "renders"
        render_dir.mkdir(exist_ok=True)
        scene.render.filepath = str(render_dir / "electric-jelly.png")
        bpy.ops.render.render(write_still=True)
        scene.render.film_transparent = True
        scene.render.filepath = str(render_dir / "electric-jelly-transparent.png")
        bpy.ops.render.render(write_still=True)
        scene.render.film_transparent = False
        scene.render.filepath = "//renders/electric-jelly.png"
        preview = bpy.data.images.load(str(render_dir / "electric-jelly.png"), check_existing=True)
        preview.name = "Electric Jelly / hero preview"
        preview.use_fake_user = True
        preview.pack()
        for area in bpy.data.screens["Rendering"].areas:
            if area.type == "IMAGE_EDITOR":
                area.spaces.active.image = preview
        bpy.ops.wm.save_as_mainfile(filepath=str(args.output), compress=True)
