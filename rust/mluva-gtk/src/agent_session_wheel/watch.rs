//! Painting the watch with GtkSnapshot from layers baked in Blender, plus the live parts.
//!
//! The case, the dial, the applied indices, the cap and the crystal come from renders in
//! `resources/wheel`: see its README. They never move while the choice swings, so each is
//! composited once into a layer, baked into a texture at the display's scale and merely appended
//! each frame. Per frame only the sunray sheen, the light, the lume bloom, the hand and its
//! shadow are drawn, so the choice can swing smoothly even on the cairo renderer.

use super::palette::Palette;
use gtk::gdk::RGBA;
use gtk::prelude::*;
use gtk::{gdk, graphene, gsk};
use std::rc::Rc;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Ready,
    Working,
    Blocked,
    /// Present, but nothing can be sent: recording keeps an editable draft.
    Draft,
    Gone,
}

/// What the watch shows: its size, one index per session and the choice.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Face {
    pub dial: f64,
    pub marks: Vec<(f64, Kind)>,
    pub chosen: Option<usize>,
    /// The choice cannot be recorded, so the light turns to the warning colour.
    pub warn: bool,
}

/// The renders are 2048 px square with the dial's radius at this many pixels.
const BAKED_DIAL: f64 = 753.6;
const FRAME: f64 = 2048.;
/// Proportions of the baked watch, in render pixels.
const CASE_OUTER: f64 = 940.;
const INDEX_INNER: f64 = 583.4;
const INDEX_OUTER: f64 = 688.8;
const INDEX_MID: f64 = 636.1;
const LUME_WIDTH: f64 = 9.7;
const LUME_INNER: f64 = 595.6;
const LUME_OUTER: f64 = 676.6;
const DOT_RADIUS_AT: f64 = 555.;
const CHAPTER_INNER: f64 = 705.;
const CELL: f64 = 128.;
const ATLAS_COLUMNS: usize = 6;
const CELLS: usize = 24;
/// The baked contact shadow reaches the render's edge, so it is faded out over this stretch of
/// the frame's half width.
const SHADOW_FADE: (f32, f32) = (0.93, 1.0);
/// The hand layers are cropped to this box of the render: left, top, width, height from the centre.
const HAND_BOX: (f64, f64, f64, f64) = (-80., -600., 160., 776.);
const SHADOW_OFFSET: (f64, f64) = (14., 24.);
/// Light comes from the upper left.
const LIGHT_BEARING: f64 = 315.;

/// The case's outer radius is this many dials.
pub const CASE_RATIO: f64 = CASE_OUTER / BAKED_DIAL;

pub fn case_radius(dial: f64) -> f64 {
    dial * CASE_RATIO
}

impl Face {
    fn scale(&self) -> f64 {
        self.dial / BAKED_DIAL
    }
    pub fn case(&self) -> f64 {
        case_radius(self.dial)
    }
    pub fn inner(&self) -> f64 {
        INDEX_INNER * self.scale()
    }
    pub fn outer(&self) -> f64 {
        INDEX_OUTER * self.scale()
    }
    /// Where the light lands on the dial at `degrees`.
    fn landing(&self, degrees: f64) -> (f64, f64) {
        point(degrees, INDEX_MID * self.scale())
    }
    /// The area every baked layer covers, centred on the watch.
    pub fn frame(&self) -> graphene::Rect {
        let half = FRAME / 2. * self.scale();
        rect(-half, -half, 2. * half, 2. * half)
    }
}

/// The baked textures, decoded once per process.
pub struct Assets {
    case: gdk::Texture,
    dial: gdk::Texture,
    sheen: gdk::Texture,
    crystal: gdk::Texture,
    atlas: gdk::Texture,
    hollow: gdk::Texture,
    hand: gdk::Texture,
    facet_left: gdk::Texture,
    facet_right: gdk::Texture,
    lume: gdk::Texture,
    shadow: gdk::Texture,
    cap: gdk::Texture,
}

macro_rules! texture {
    ($name:literal) => {
        gdk::Texture::from_bytes(&glib::Bytes::from_static(include_bytes!(concat!(
            "../../resources/wheel/",
            $name
        ))))
        .expect(concat!("bundled wheel texture ", $name))
    };
}

pub fn assets() -> Rc<Assets> {
    thread_local!(static ASSETS: Rc<Assets> = Rc::new(Assets {
        case: texture!("case.png"),
        dial: texture!("dial-luma.png"),
        sheen: texture!("sheen.png"),
        crystal: texture!("crystal.png"),
        atlas: texture!("index-atlas.png"),
        hollow: texture!("index-hollow-atlas.png"),
        hand: texture!("hand.png"),
        facet_left: texture!("hand-facet-left.png"),
        facet_right: texture!("hand-facet-right.png"),
        lume: texture!("hand-lume.png"),
        shadow: texture!("hand-shadow.png"),
        cap: texture!("cap.png"),
    }));
    ASSETS.with(Rc::clone)
}

pub fn point(degrees: f64, radius: f64) -> (f64, f64) {
    let angle = degrees.to_radians();
    (radius * angle.sin(), -radius * angle.cos())
}

fn rect(x: f64, y: f64, w: f64, h: f64) -> graphene::Rect {
    graphene::Rect::new(x as f32, y as f32, w as f32, h as f32)
}
fn at(x: f64, y: f64) -> graphene::Point {
    graphene::Point::new(x as f32, y as f32)
}
fn disc(radius: f64) -> gsk::RoundedRect {
    gsk::RoundedRect::from_rect(
        rect(-radius, -radius, 2. * radius, 2. * radius),
        radius as f32,
    )
}
fn square(radius: f64) -> graphene::Rect {
    rect(-radius, -radius, 2. * radius, 2. * radius)
}
fn stop(offset: f32, color: RGBA) -> gsk::ColorStop {
    gsk::ColorStop::new(offset, color)
}
fn clear() -> RGBA {
    RGBA::new(0., 0., 0., 0.)
}

fn fill(snapshot: &gtk::Snapshot, path: &gsk::Path, color: &RGBA) {
    snapshot.append_fill(path, gsk::FillRule::Winding, color);
}

fn clipped_to(snapshot: &gtk::Snapshot, radius: f64, paint: impl FnOnce(&gtk::Snapshot)) {
    snapshot.push_rounded_clip(&disc(radius));
    paint(snapshot);
    snapshot.pop();
}

fn draw(snapshot: &gtk::Snapshot, texture: &gdk::Texture, bounds: &graphene::Rect) {
    snapshot.append_scaled_texture(texture, gsk::ScalingFilter::Trilinear, bounds);
}

/// Draw `paint` with every colour channel multiplied by `rgb`, alpha by `alpha`.
fn tinted(snapshot: &gtk::Snapshot, rgb: [f32; 3], alpha: f32, paint: impl FnOnce(&gtk::Snapshot)) {
    let matrix = graphene::Matrix::from_float([
        rgb[0], 0., 0., 0., 0., rgb[1], 0., 0., 0., 0., rgb[2], 0., 0., 0., 0., alpha,
    ]);
    snapshot.push_color_matrix(&matrix, &graphene::Vec4::zero());
    paint(snapshot);
    snapshot.pop();
}

/// The hand layers' box, scaled to the watch.
fn hand_box(scale: f64) -> graphene::Rect {
    rect(
        HAND_BOX.0 * scale,
        HAND_BOX.1 * scale,
        HAND_BOX.2 * scale,
        HAND_BOX.3 * scale,
    )
}

/// The case with its contact shadow, and the dial tinted to the theme.
pub fn under(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let assets = assets();
    let frame = face.frame();
    let half = (FRAME / 2. * face.scale()) as f32;
    snapshot.push_mask(gsk::MaskMode::Alpha);
    snapshot.append_radial_gradient(
        &frame,
        &at(0., 0.),
        half,
        half,
        0.,
        1.,
        &[
            stop(0., RGBA::new(0., 0., 0., 1.)),
            stop(SHADOW_FADE.0, RGBA::new(0., 0., 0., 1.)),
            stop(SHADOW_FADE.1, clear()),
            stop(1., clear()),
        ],
    );
    snapshot.pop();
    draw(snapshot, &assets.case, &frame);
    snapshot.pop();
    // The dial is a luminance map: colour = tint · luma / 0.5, so a mean luma lands on the tint.
    let tint = p.dial_tint;
    tinted(snapshot, [tint.0 * 2., tint.1 * 2., tint.2 * 2.], 1., |s| {
        draw(s, &assets.dial, &frame)
    });
}

/// The sunray sheen: a bright bar through the centre that turns to face the light.
pub fn sheen(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, sun: f64, strength: f64) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let scale = face.scale();
    clipped_to(snapshot, CHAPTER_INNER * scale, |s| {
        s.save();
        s.rotate(sun as f32);
        s.push_opacity(0.55 * strength);
        tinted(s, [lit.0, lit.1, lit.2], 1., |s| {
            draw(s, &assets().sheen, &face.frame())
        });
        s.pop();
        s.restore();
    });
}

/// The light: a soft wedge from the centre toward the choice and a pool where it lands.
pub fn light(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, sun: f64, glow: f64) {
    if glow <= 0.01 {
        return;
    }
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let glow = glow as f32;
    let dial = face.dial;
    clipped_to(snapshot, dial, |s| {
        s.push_opacity(f64::from(glow));
        s.push_mask(gsk::MaskMode::Alpha);
        let reach = (dial * std::f64::consts::SQRT_2) as f32;
        s.append_radial_gradient(
            &square(dial),
            &at(0., 0.),
            reach,
            reach,
            0.,
            1.,
            &[
                stop(0., clear()),
                stop(0.14, clear()),
                stop(0.46, RGBA::new(0., 0., 0., 1.)),
                stop(0.84, RGBA::new(0., 0., 0., 1.)),
                stop(1., clear()),
            ],
        );
        s.pop();
        let tint = |alpha: f32| lit.alpha(alpha);
        s.append_conic_gradient(
            &square(dial),
            &at(0., 0.),
            (sun - 16.).rem_euclid(360.) as f32,
            &[
                stop(0., clear()),
                stop(10. / 360., tint(0.16)),
                stop(16. / 360., tint(0.44)),
                stop(22. / 360., tint(0.16)),
                stop(32. / 360., clear()),
                stop(1., clear()),
            ],
        );
        s.pop();
        let (x, y) = face.landing(sun);
        // The pool is as wide as the sixth part of the dial, so it keeps its look at any size.
        let reach = (220. * face.scale()).max(60.);
        s.append_radial_gradient(
            &rect(x - reach, y - reach, 2. * reach, 2. * reach),
            &at(x, y),
            reach as f32,
            reach as f32,
            0.,
            1.,
            &[
                stop(0., tint(0.8)),
                stop(0.34, tint(0.3)),
                stop(0.68, clear()),
                stop(1., clear()),
            ],
        );
        s.pop();
    });
}

/// A light source blooming: a wide halo, a near one and the source itself.
pub fn bloom(snapshot: &gtk::Snapshot, strength: f64, paint: impl Fn(&gtk::Snapshot)) {
    if strength <= 0.01 {
        return;
    }
    snapshot.push_opacity(strength.min(1.));
    for radius in [10., 10., 4.] {
        snapshot.push_blur(radius);
        paint(snapshot);
        snapshot.pop();
    }
    paint(snapshot);
    snapshot.pop();
}

/// One baked index: the nearest of the 24 views, turned by the few degrees left over. Lighting is
/// fixed in screen space, so a view is never turned further than half a step.
fn index_view(snapshot: &gtk::Snapshot, atlas: &gdk::Texture, face: &Face, angle: f64) {
    let scale = face.scale();
    let nearest = (angle.rem_euclid(360.) / 15.).round();
    let residual = angle.rem_euclid(360.) - nearest * 15.;
    let index = nearest as usize % CELLS;
    let (column, row) = (index % ATLAS_COLUMNS, index / ATLAS_COLUMNS);
    let (cx, cy) = point(angle, INDEX_MID * scale);
    let cell = CELL * scale;
    snapshot.save();
    snapshot.translate(&at(cx, cy));
    snapshot.rotate(residual as f32);
    snapshot.push_clip(&rect(-cell / 2., -cell / 2., cell, cell));
    draw(
        snapshot,
        atlas,
        &rect(
            -(column as f64 + 0.5) * cell,
            -(row as f64 + 0.5) * cell,
            768. * scale,
            512. * scale,
        ),
    );
    snapshot.pop();
    snapshot.restore();
}

/// The recess down the middle of an index, filled with `color`.
fn lume_channel(snapshot: &gtk::Snapshot, face: &Face, angle: f64, color: &RGBA, widen: f64) {
    let scale = face.scale();
    let width = LUME_WIDTH * scale * widen;
    snapshot.save();
    snapshot.rotate(angle as f32);
    snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
        rect(
            -width / 2.,
            -LUME_OUTER * scale,
            width,
            (LUME_OUTER - LUME_INNER) * scale,
        ),
        (width / 2.) as f32,
    ));
    snapshot.append_color(
        color,
        &rect(
            -width,
            -LUME_OUTER * scale,
            2. * width,
            (LUME_OUTER - LUME_INNER) * scale,
        ),
    );
    snapshot.pop();
    snapshot.restore();
}

fn dot_at(face: &Face, angle: f64) -> (f64, f64, f64) {
    let scale = face.scale();
    let (x, y) = point(angle, DOT_RADIUS_AT * scale);
    (x, y, (6. * scale).max(2.5))
}

/// The applied indices and their dim lume, set once. A working index has its dot and a blocked
/// one amber lume, both blooming; a draft is hollow; a gone session leaves an empty setting.
pub fn marks(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let assets = assets();
    let scale = face.scale();
    let dim = p.steel.alpha(0.42);
    for (angle, kind) in &face.marks {
        match kind {
            Kind::Gone => {
                // An empty setting: two tiny feet where the index was.
                for radius in [INDEX_OUTER - 8., INDEX_INNER + 8.] {
                    let (x, y) = point(*angle, radius * scale);
                    let foot = gsk::PathBuilder::new();
                    foot.add_circle(&at(x, y), (2. * scale).max(1.4) as f32);
                    fill(snapshot, &foot.to_path(), &p.darker.solid());
                }
            }
            Kind::Draft => index_view(snapshot, &assets.hollow, face, *angle),
            Kind::Ready | Kind::Working | Kind::Blocked => {
                index_view(snapshot, &assets.atlas, face, *angle);
                if *kind == Kind::Blocked {
                    let warn = p.warning.solid();
                    lume_channel(snapshot, face, *angle, &warn, 1.);
                    bloom(snapshot, 0.8, |s| lume_channel(s, face, *angle, &warn, 1.));
                } else {
                    lume_channel(snapshot, face, *angle, &dim, 1.);
                }
                if *kind == Kind::Working {
                    let (x, y, radius) = dot_at(face, *angle);
                    let dot = |s: &gtk::Snapshot| {
                        let path = gsk::PathBuilder::new();
                        path.add_circle(&at(x, y), radius as f32);
                        fill(s, &path.to_path(), &p.working.solid());
                    };
                    dot(snapshot);
                    bloom(snapshot, 0.8, dot);
                }
            }
        }
    }
}

/// The chosen index lit by the light: its lume in the light's colour.
pub fn lit_index(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64, kind: Kind) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let scale = face.scale();
    if kind == Kind::Gone {
        let (x, y) = point(angle, INDEX_MID * scale);
        let ring = gsk::PathBuilder::new();
        ring.add_circle(&at(x, y), (8. * scale).max(3.) as f32);
        snapshot.append_stroke(&ring.to_path(), &gsk::Stroke::new(1.5), &lit.solid());
    } else {
        let widen = if kind == Kind::Draft { 1.8 } else { 1.5 };
        lume_channel(snapshot, face, angle, &lit.solid(), widen);
    }
}

/// The faceted hand and its shadow on the dial. The lit and shaded facets trade places as it
/// turns, against the fixed light from the upper left.
pub fn hand(snapshot: &gtk::Snapshot, face: &Face, angle: f64) {
    let assets = assets();
    let scale = face.scale();
    let bounds = hand_box(scale);
    // The shadow falls to the lower right on the screen, whatever way the hand points.
    snapshot.save();
    snapshot.translate(&at(SHADOW_OFFSET.0 * scale, SHADOW_OFFSET.1 * scale));
    snapshot.rotate(angle as f32);
    tinted(snapshot, [0., 0., 0.], 0.6, |s| {
        draw(s, &assets.shadow, &bounds)
    });
    snapshot.restore();
    snapshot.save();
    snapshot.rotate(angle as f32);
    draw(snapshot, &assets.hand, &bounds);
    for (facet, normal) in [
        (&assets.facet_left, angle - 90.),
        (&assets.facet_right, angle + 90.),
    ] {
        let facing = (normal - LIGHT_BEARING).to_radians().cos() as f32;
        if facing > 0. {
            snapshot.push_opacity(f64::from(0.45 * facing));
            draw(snapshot, facet, &bounds);
            snapshot.pop();
        } else {
            tinted(snapshot, [0., 0., 0.], 0.5 * -facing, |s| {
                draw(s, facet, &bounds)
            });
        }
    }
    snapshot.restore();
}

/// The lume channel down the hand, glowing.
pub fn hand_lume(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let bounds = hand_box(face.scale());
    bloom(snapshot, 0.9, |s| {
        s.save();
        s.rotate(angle as f32);
        tinted(s, [lit.0, lit.1, lit.2], 1., |s| {
            draw(s, &assets().lume, &bounds)
        });
        s.restore();
    });
}

/// The polished pin where the hand turns.
pub fn cap(snapshot: &gtk::Snapshot, face: &Face) {
    draw(snapshot, &assets().cap, &hand_box(face.scale()));
}

/// The domed crystal with its Fresnel reflections, over everything on the dial.
pub fn crystal(snapshot: &gtk::Snapshot, face: &Face) {
    draw(snapshot, &assets().crystal, &face.frame());
}

/// A tile of faint noise that dithers the room's lift, so it never bands.
pub fn grain() -> gdk::Texture {
    const SIDE: usize = 128;
    let mut state = 0x2545_f491_4f6c_dd1du64;
    let mut pixels = Vec::with_capacity(SIDE * SIDE * 4);
    for _ in 0..SIDE * SIDE {
        state ^= state << 13;
        state ^= state >> 7;
        state ^= state << 17;
        let level = (state >> 56) as u8;
        // Premultiplied grey at a low alpha: half the tile lightens, half darkens.
        let alpha = 9u8;
        let grey = if level > 127 { alpha } else { 0 };
        pixels.extend_from_slice(&[grey, grey, grey, alpha]);
    }
    gdk::MemoryTexture::new(
        SIDE as i32,
        SIDE as i32,
        gdk::MemoryFormat::R8g8b8a8Premultiplied,
        &glib::Bytes::from_owned(pixels),
        SIDE * 4,
    )
    .upcast()
}

/// The opaque room: one ground, a faint lift around the watch and grain over both.
pub fn room(
    snapshot: &gtk::Snapshot,
    p: &Palette,
    (width, height): (f64, f64),
    centre: (f64, f64),
    grain: &gdk::Texture,
) {
    let bounds = rect(0., 0., width, height);
    snapshot.append_color(&p.room.solid(), &bounds);
    let reach = (height.max(width) * 0.46) as f32;
    snapshot.append_radial_gradient(
        &bounds,
        &at(centre.0, centre.1),
        reach,
        reach,
        0.,
        1.,
        &[stop(0., p.room_lift.solid()), stop(1., p.room.alpha(0.))],
    );
    snapshot.push_repeat(&bounds, None);
    snapshot.append_texture(grain, &rect(0., 0., 128., 128.));
    snapshot.pop();
}

/// Smallest angle between two bearings.
pub fn separation(a: f64, b: f64) -> f64 {
    ((a - b + 540.).rem_euclid(360.) - 180.).abs()
}

/// How strongly the light on `sun` shows on an index at `angle`: full on it, none 40° away.
pub fn proximity(sun: f64, angle: f64) -> f64 {
    let t = (1. - separation(sun, angle) / 40.).clamp(0., 1.);
    t * t * (3. - 2. * t)
}
