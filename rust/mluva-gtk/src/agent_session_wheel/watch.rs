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
/// Brightness gain on the case layer.
const CASE_GAIN: f32 = 1.6;
const SHADOW_FADE: (f32, f32) = (0.93, 1.0);
/// The hand layers are cropped to this box of the render: left, top, width, height from the centre.
const HAND_BOX: (f64, f64, f64, f64) = (-80., -600., 160., 776.);
/// The hand is drawn this much slimmer than baked, and the cap this much smaller, to match the
/// render's proportions against the dial.
const HAND_SLIM: f32 = 0.8;
/// The hub's radius, in dials.
const CAP_RADIUS: f64 = 0.035;
/// The hand's tail past the centre, in hand lengths.
const TAIL: f64 = 0.17;
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
    /// The wider area of the case's soft shadow.
    pub fn shadow_frame(&self) -> graphene::Rect {
        let half = FRAME / 2. * self.scale() * 1.9;
        rect(-half, -half, 2. * half, 2. * half)
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

/// Draw `paint` with every colour channel scaled by `gain` and raised by `offset`.
fn lifted(snapshot: &gtk::Snapshot, gain: f32, offset: f32, paint: impl FnOnce(&gtk::Snapshot)) {
    let matrix = graphene::Matrix::from_float([
        gain, 0., 0., 0., 0., gain, 0., 0., 0., 0., gain, 0., 0., 0., 0., 1.,
    ]);
    snapshot.push_color_matrix(&matrix, &graphene::Vec4::new(offset, offset, offset, 0.));
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

/// A large soft shadow under the case, falling to the lower right.
pub fn shadow(snapshot: &gtk::Snapshot, face: &Face) {
    let scale = face.scale();
    let case = face.case();
    snapshot.append_outset_shadow(
        &disc(case),
        &RGBA::new(0., 0., 0., 0.6),
        (70. * scale) as f32,
        (130. * scale) as f32,
        0.,
        (260. * scale) as f32,
    );
    snapshot.append_outset_shadow(
        &disc(case),
        &RGBA::new(0., 0., 0., 0.45),
        (14. * scale) as f32,
        (26. * scale) as f32,
        0.,
        (50. * scale) as f32,
    );
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
    // The case is lifted so its steel reads as lit metal; the gasket stays darker than it.
    lifted(snapshot, CASE_GAIN, 0.03, |s| draw(s, &assets.case, &frame));
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
                stop(0.106, clear()),
                stop(0.66, RGBA::new(0., 0., 0., 1.)),
                stop(1., RGBA::new(0., 0., 0., 1.)),
            ],
        );
        s.pop();
        let tint = |alpha: f32| lit.alpha(alpha);
        // A wedge about 12° wide that fans out toward the rim, feathered at its sides.
        s.append_conic_gradient(
            &square(dial),
            &at(0., 0.),
            (sun - 14.).rem_euclid(360.) as f32,
            &[
                stop(0., clear()),
                stop(3. / 360., tint(0.12)),
                stop(8. / 360., tint(0.65)),
                stop(14. / 360., tint(0.75)),
                stop(20. / 360., tint(0.65)),
                stop(25. / 360., tint(0.12)),
                stop(28. / 360., clear()),
                stop(1., clear()),
            ],
        );
        s.pop();
        let (x, y) = face.landing(sun);
        // The pool is as wide as the sixth part of the dial, so it keeps its look at any size.
        let reach = (270. * face.scale()).max(70.);
        s.append_radial_gradient(
            &rect(x - reach, y - reach, 2. * reach, 2. * reach),
            &at(x, y),
            reach as f32,
            reach as f32,
            0.,
            1.,
            &[
                stop(0., tint(0.9)),
                stop(0.34, tint(0.4)),
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
    // The polished steel is lifted a little so each baton reads as one bright piece.
    lifted(snapshot, 1.18, 0.0, |s| {
        draw(
            s,
            atlas,
            &rect(
                -(column as f64 + 0.5) * cell,
                -(row as f64 + 0.5) * cell,
                768. * scale,
                512. * scale,
            ),
        )
    });
    snapshot.pop();
    snapshot.restore();
}

/// The recess down the middle of an index, filled with `color`.
fn lume_channel(snapshot: &gtk::Snapshot, face: &Face, angle: f64, color: &RGBA, widen: f64) {
    let scale = face.scale();
    // A hair narrower than the recess, so a thin dark outline remains, with a fainter wider pass
    // for a soft edge.
    let core = (LUME_WIDTH * scale * widen - 1.5).max(1.6);
    snapshot.save();
    snapshot.rotate(angle as f32);
    for (width, alpha) in [(core + 1.2, 0.35), (core, 1.)] {
        let length = (LUME_OUTER - LUME_INNER) * scale;
        snapshot.push_rounded_clip(&gsk::RoundedRect::from_rect(
            rect(-width / 2., -LUME_OUTER * scale, width, length),
            (width / 2.) as f32,
        ));
        let faded = RGBA::new(
            color.red(),
            color.green(),
            color.blue(),
            color.alpha() * alpha,
        );
        snapshot.append_color(
            &faded,
            &rect(-width, -LUME_OUTER * scale, 2. * width, length),
        );
        snapshot.pop();
    }
    snapshot.restore();
}

fn dot_at(face: &Face, angle: f64) -> (f64, f64, f64) {
    let scale = face.scale();
    let (x, y) = point(angle, DOT_RADIUS_AT * scale);
    (x, y, (8. * scale).max(3.2))
}

/// The applied indices and their dim lume, set once. A working index has its dot and a blocked
/// one amber lume, both blooming; a draft is hollow; a gone session leaves an empty setting.
pub fn marks(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let assets = assets();
    let scale = face.scale();
    let dim = p.steel.alpha(0.85);
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
                    let warn = p.warning.over(p.steel, 0.85).solid();
                    lume_channel(snapshot, face, *angle, &warn, 1.);
                    for _ in 0..2 {
                        bloom(snapshot, 1., |s| lume_channel(s, face, *angle, &warn, 1.4));
                    }
                } else {
                    lume_channel(snapshot, face, *angle, &dim, 1.);
                }
                if *kind == Kind::Working {
                    let (x, y, radius) = dot_at(face, *angle);
                    let dot = |s: &gtk::Snapshot| {
                        let path = gsk::PathBuilder::new();
                        path.add_circle(&at(x, y), radius as f32);
                        fill(s, &path.to_path(), &p.working.over(p.steel, 0.7).solid());
                    };
                    dot(snapshot);
                    for _ in 0..2 {
                        bloom(snapshot, 1., dot);
                    }
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

/// The hand's box cut short past the centre: only a deliberate counterweight remains.
fn tail_clip(scale: f64) -> graphene::Rect {
    let b = HAND_BOX;
    rect(
        b.0 * scale,
        b.1 * scale,
        b.2 * scale,
        (-b.1 + TAIL * 567.2) * scale,
    )
}

/// The faceted hand and its shadow on the dial. The lit and shaded facets trade places as it
/// turns, against the fixed light from the upper left.
pub fn hand(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64) {
    let assets = assets();
    let scale = face.scale();
    let bounds = hand_box(scale);
    // The shadow falls to the lower right on the screen, whatever way the hand points.
    snapshot.save();
    snapshot.translate(&at(SHADOW_OFFSET.0 * scale, SHADOW_OFFSET.1 * scale));
    snapshot.rotate(angle as f32);
    snapshot.scale(HAND_SLIM, 1.);
    snapshot.push_clip(&tail_clip(scale));
    tinted(snapshot, [0., 0., 0.], 0.6, |s| {
        draw(s, &assets.shadow, &bounds)
    });
    snapshot.pop();
    snapshot.restore();
    snapshot.save();
    snapshot.rotate(angle as f32);
    snapshot.scale(HAND_SLIM, 1.);
    snapshot.push_clip(&tail_clip(scale));
    draw(snapshot, &assets.hand, &bounds);
    // The facet toward the light goes near white, the other near the dial's navy.
    let navy = p.dial_tint;
    for (facet, normal) in [
        (&assets.facet_left, angle - 90.),
        (&assets.facet_right, angle + 90.),
    ] {
        let facing = (normal - LIGHT_BEARING).to_radians().cos() as f32;
        let strength = facing.abs().powf(0.6);
        if facing > 0. {
            tinted(snapshot, [1., 1., 1.], 0.85 * strength, |s| {
                draw(s, facet, &bounds)
            });
        } else {
            tinted(snapshot, [navy.0, navy.1, navy.2], 0.9 * strength, |s| {
                draw(s, facet, &bounds)
            });
        }
    }
    snapshot.pop();
    snapshot.restore();
}

/// The lume channel down the hand, glowing.
pub fn hand_lume(snapshot: &gtk::Snapshot, p: &Palette, face: &Face, angle: f64) {
    let lit = if face.warn { p.warn_lit } else { p.lit };
    let bounds = hand_box(face.scale());
    bloom(snapshot, 1., |s| {
        s.save();
        s.rotate(angle as f32);
        s.scale(HAND_SLIM, 1.);
        tinted(s, [lit.0, lit.1, lit.2], 1., |s| {
            draw(s, &assets().lume, &bounds)
        });
        s.restore();
    });
}

/// The hub: a small neutral polished-steel cap. A thin bevelled rim bright at the upper left and
/// dark at the lower right, a domed face with one soft highlight, a tiny dark pinion hole, and a
/// single blurred shadow toward the lower right, as the hand's.
pub fn cap(snapshot: &gtk::Snapshot, p: &Palette, face: &Face) {
    let radius = CAP_RADIUS * face.dial;
    let grey = |level: f32| {
        let steel = p.steel.luma();
        RGBA::new(steel * level, steel * level, steel * level, 1.)
    };
    snapshot.append_outset_shadow(
        &disc(radius),
        &RGBA::new(0., 0., 0., 0.6),
        (0.45 * radius) as f32,
        (0.8 * radius) as f32,
        0.,
        (1.4 * radius) as f32,
    );
    snapshot.push_rounded_clip(&disc(radius));
    snapshot.append_linear_gradient(
        &square(radius),
        &at(-radius * 0.7, -radius * 0.7),
        &at(radius * 0.7, radius * 0.7),
        &[
            stop(0., grey(1.)),
            stop(0.5, grey(0.55)),
            stop(1., grey(0.16)),
        ],
    );
    snapshot.pop();
    let dome = radius * 0.76;
    snapshot.push_rounded_clip(&disc(dome));
    snapshot.append_radial_gradient(
        &square(dome),
        &at(-dome * 0.3, -dome * 0.35),
        (dome * 1.5) as f32,
        (dome * 1.5) as f32,
        0.,
        1.,
        &[
            stop(0., grey(0.95)),
            stop(0.5, grey(0.6)),
            stop(1., grey(0.26)),
        ],
    );
    snapshot.pop();
    let shine = dome * 0.45;
    snapshot.append_radial_gradient(
        &rect(
            -dome * 0.3 - shine,
            -dome * 0.35 - shine,
            2. * shine,
            2. * shine,
        ),
        &at(-dome * 0.3, -dome * 0.35),
        shine as f32,
        shine as f32,
        0.,
        1.,
        &[stop(0., RGBA::new(1., 1., 1., 0.85)), stop(1., clear())],
    );
    let pinion = gsk::PathBuilder::new();
    pinion.add_circle(&at(0., 0.), (radius * 0.12).max(0.9) as f32);
    fill(
        snapshot,
        &pinion.to_path(),
        &RGBA::new(0.03, 0.03, 0.04, 1.),
    );
}

/// The domed crystal with its Fresnel reflections, over everything on the dial.
pub fn crystal(snapshot: &gtk::Snapshot, face: &Face) {
    draw(snapshot, &assets().crystal, &face.frame());
}

/// A tile of noise of one level, invisible to the eye, that dithers the room's lift so it never bands.
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
        let alpha = 1u8;
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
    // A soft vignette: the edges fall away to a darker slate.
    let far = (width.hypot(height) / 2.) as f32;
    snapshot.append_radial_gradient(
        &bounds,
        &at(centre.0, centre.1),
        far,
        far,
        0.,
        1.,
        &[
            stop(0., clear()),
            stop(0.5, clear()),
            stop(1., RGBA::new(0., 0., 0., 0.4)),
        ],
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
