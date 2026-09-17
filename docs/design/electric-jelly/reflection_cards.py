"""Place editable studio reflectors from art-directed highlight footprints."""

import json
import re

import bpy
from mathutils import Vector
from mathutils.geometry import barycentric_transform, tessellate_polygon


def sample_path(data):
    """Sample the absolute M/L/C/Z paths used by the reflection guide."""
    tokens = re.findall(r"[MCLZ]|-?\d+(?:\.\d+)?(?:e[-+]?\d+)?", data)
    points = []
    index = 0
    point = Vector((0, 0, 0))
    while index < len(tokens):
        command = tokens[index]
        index += 1
        if command in ("M", "L"):
            point = Vector((float(tokens[index]), float(tokens[index + 1]), 0))
            index += 2
            points.append(point)
        elif command == "C":
            controls = [Vector((float(tokens[index + j]), float(tokens[index + j + 1]), 0)) for j in (0, 2, 4)]
            index += 6
            first, second, end = controls
            for step in range(1, 13):
                t = step / 12
                points.append(
                    (1 - t) ** 3 * point + 3 * (1 - t) ** 2 * t * first + 3 * (1 - t) * t * t * second + t**3 * end
                )
            point = end
        elif command == "Z":
            break
        else:
            raise ValueError(f"Unsupported guide command: {command}")
    if (points[-1] - points[0]).length < 0.001:
        points.pop()
    return points


def subdivide(triangle, depth):
    """Keep reflector triangles small enough to follow the curved reflection field."""
    if depth == 0:
        return [triangle]
    a, b, c = triangle
    ab, bc, ca = (a + b) * 0.5, (b + c) * 0.5, (c + a) * 0.5
    return [part for tri in ((a, ab, ca), (ab, b, bc), (ca, bc, c), (ab, bc, ca)) for part in subdivide(tri, depth - 1)]


def smooth_normal(mesh, location, polygon_index):
    """Use interpolated surface normals so the studio cards do not inherit mesh facets."""
    vertices = [mesh.vertices[index] for index in mesh.polygons[polygon_index].vertices]
    basis = (Vector((1, 0, 0)), Vector((0, 1, 0)), Vector((0, 0, 1)))
    for index in range(1, len(vertices) - 1):
        triangle = (vertices[0], vertices[index], vertices[index + 1])
        weights = barycentric_transform(location, *(vertex.co for vertex in triangle), *basis)
        if min(weights) >= -0.001:
            return sum((vertex.normal * weight for vertex, weight in zip(triangle, weights)), Vector()).normalized()
    raise ValueError("Reflection guide hit did not lie inside its surface polygon.")


def create_reflectors(logo, collection, guide_path):
    """Reflect camera rays off the logo to locate shaped luminous studio panels.

    These are actual geometry outside the logo, visible to glossy rays. The main
    camera cannot see them directly. Their shapes are tuned for the front hero view.
    """
    bpy.context.view_layer.update()
    evaluated = logo.evaluated_get(bpy.context.evaluated_depsgraph_get())
    for card in json.loads(guide_path.read_text()):
        points = sample_path(card["path"])
        triangles = [tuple(points[i] for i in tri) for tri in tessellate_polygon([points])]
        triangles = [part for tri in triangles for part in subdivide(tri, 3)]
        vertices, faces, weights = [], [], []
        for triangle in triangles:
            mapped, intensities = [], []
            for point in triangle:
                px = card["origin"][0] + point.x * card["scale"][0]
                py = card["origin"][1] + point.y * card["scale"][1]
                hit, location, normal, polygon_index = evaluated.ray_cast(
                    ((px - 200) / 100, (200 - py) / 100, 3), (0, 0, -1)
                )
                if not hit:
                    break  # Clip the broad glow guide to the actual silhouette.
                normal = smooth_normal(evaluated.data, location, polygon_index)
                reflection = 2 * normal * normal.z - Vector((0, 0, 1))
                mapped.append(location + reflection * 8)
                fraction = point.y / card["size"][1]
                if card["falloff"] == "crown":
                    weight = 0.03 + 0.97 * max(0, 1 - fraction) ** 1.9
                elif card["falloff"] == "radial":
                    x = (point.x / card["size"][0] - 0.5) * 2
                    y = (fraction - 0.5) * 2
                    weight = max(0, 1 - x * x - y * y) ** 2
                else:
                    weight = 0.75
                intensities.append(weight)
            if len(mapped) != 3:
                continue
            start = len(vertices)
            vertices.extend(mapped)
            weights.extend(intensities)
            faces.append((start, start + 1, start + 2))
        mesh = bpy.data.meshes.new(f"Reflector / {card['name']}")
        mesh.from_pydata(vertices, [], faces)
        mesh.update()
        falloff = mesh.color_attributes.new(name="Falloff", type="FLOAT_COLOR", domain="POINT")
        for slot, weight in zip(falloff.data, weights, strict=True):
            slot.color = tuple(weight * channel for channel in card["color"]) + (1,)
        obj = bpy.data.objects.new(f"Reflection / {card['name']}", mesh)
        collection.objects.link(obj)
        obj.visible_camera = False
        obj.visible_diffuse = False
        obj.visible_transmission = False
        obj.visible_shadow = False
        material = bpy.data.materials.new(obj.name)
        material.use_nodes = True
        nodes, links = material.node_tree.nodes, material.node_tree.links
        nodes.clear()
        attribute = nodes.new("ShaderNodeVertexColor")
        attribute.layer_name = "Falloff"
        attribute.location = (-240, 0)
        emission = nodes.new("ShaderNodeEmission")
        emission.inputs["Strength"].default_value = card["strength"]
        output = nodes.new("ShaderNodeOutputMaterial")
        output.location = (240, 0)
        links.new(attribute.outputs["Color"], emission.inputs["Color"])
        links.new(emission.outputs[0], output.inputs["Surface"])
        mesh.materials.append(material)
        obj.hide_set(True)  # Keep the modeling viewport clear; Cycles still renders the reflection cards.
