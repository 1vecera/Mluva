# Baked layers for the session wheel

The wheel's case, dial, applied indices, hand, cap and crystal are renders from Blender, composited by `src/agent_session_wheel/watch.rs` with GtkSnapshot. They are compiled in with `include_bytes!`, decoded once per process into `gdk::Texture`s and drawn with trilinear filtering, so they cannot go missing from an install.

| File | Size (px) | What it is | How the code uses it |
| --- | --- | --- | --- |
| `case.png` | 1536², straight alpha | Knurled steel case with a baked contact shadow; transparent inside the dial | Drawn first, faded out at the frame's edge |
| `dial-luma.png` | 1536², grey + alpha | The sunray dial as luminance | Tinted at runtime: colour = tint · luma / 0.5, with the tint taken from the theme's accent and darker ground |
| `sheen.png` | 1024², grey | The bright sunray bar through the centre, pointing at twelve | Screened onto the dial at about 0.55 opacity in the light's colour, turned to the light's spring angle |
| `index-atlas.png`, `index-hollow-atlas.png` | 768×512, 6×4 cells of 128² | The applied index, and the hollow draft-only index, every 15° | Nearest cell, turned by the remainder; the lume channel is a recess filled live |
| `hand.png`, `hand-facet-left.png`, `hand-facet-right.png`, `hand-lume.png`, `hand-shadow.png`, `cap.png` | 160×776 crops of the 2048² frame | The hand pointing at twelve, its facet masks, lume mask, shadow silhouette and the cap | Turned about the centre; facets lit or shaded against a fixed upper-left light; the shadow offset in screen space |
| `crystal.png` | 1024², straight alpha | The domed crystal with its Fresnel reflections | Drawn last over the dial, hands and cap |
| `manifest.json` | | Radii, angles and layer contracts from the render | The constants at the top of `watch.rs` follow it |
| `sundial-watch.blend` | 544 KB | The Blender scene | Not compiled in; the source for re-baking |

The render frame is 2048² with the centre at (1024, 1024) and the dial's radius at 753.6 px; everything is scaled by `dial radius / 753.6`. The PNGs total about 2.5 MB.

Two changes were made to the artist's files when shipping them: the full-frame layers were downscaled (the largest case on screen is about 1400 device px across), and `crystal.png`, which the artist premultiplied, is stored with straight alpha so that GTK, which premultiplies on load, ends with the same pixels. The hand layers were cropped to the box that holds them (x −80…80, y −600…176 from the centre).

## How they were baked

`sundial-watch.blend` is rendered with Cycles at 128 samples and the AgX view transform, so every layer is display-referred sRGB and must not be tone-mapped again. The scene's camera, `DialCam`, is orthographic and looks straight down from z = 0.3 m (the saved scene frames the 1920×1200 hero view with an orthographic scale of 0.096). The layers are square 2048² renders with a transparent film, framed so that the dial radius is 753.6 px (`px_per_cm` in `manifest.json`). Light comes from a spot over the dial (`SunSpot`), a key area light at the upper left, a rim, a kick and a faint top light. Each layer is the render of one group of the scene's objects: `CaseBody`, `BezelKnurl`, `BezelTop` and `Gasket` for the case; `Dial`, `ChapterRing` and `MinuteTrack` for the dial; `IndexNN` and `LumeNN` for the atlases, once per 15° step; the hand, its lume and the cap; and `Crystal`. The dial was divided by its mean luminance gain (4.3176) to make `dial-luma.png`.

To re-bake, open the `.blend`, hide everything but the layer's objects, set the output to 2048² with a transparent film, render with the camera above and save as PNG. The scripts that isolated the layers were not kept, so a re-bake is by hand; keep the radii in `manifest.json` unchanged, or update the constants in `watch.rs` to match, then downscale as above.
