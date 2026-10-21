//! Draw keyframes: crop to the activity, add the ink and a `#N M:SS` badge, and lay out the
//! contact sheet. Cairo and Pango run on the calling thread; nothing here touches GTK.
use crate::model::{Frame, Keyframe, Mark, Screen, Shape, Stroke, format_clock};
use cairo::{Context, Format, ImageSurface};
use gdk_pixbuf::prelude::*;
use std::{
    fs::OpenOptions,
    io::{self, Cursor, Read},
    os::unix::fs::OpenOptionsExt,
    path::Path,
};

/// Long edge of a rendered keyframe and of the contact sheet, in pixels.
pub const MAX_EDGE: f64 = 1600.0;
/// Captured frames larger than this are refused instead of decoded.
const MAX_FRAME_BYTES: u64 = 64 * 1024 * 1024;
const INK: (f64, f64, f64) = (1.0, 0.231, 0.188);

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("{0}")]
    Io(#[from] io::Error),
    #[error("Could not draw a keyframe: {0}")]
    Cairo(#[from] cairo::Error),
    #[error("Could not read a captured frame.")]
    Frame,
}

/// Decode a private PNG or JPEG frame without following links.
pub fn load(path: &Path) -> Result<ImageSurface, RenderError> {
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)
        .open(path)?;
    if !file.metadata()?.is_file() {
        return Err(RenderError::Frame);
    }
    let mut data = Vec::new();
    (&mut file)
        .take(MAX_FRAME_BYTES + 1)
        .read_to_end(&mut data)?;
    if data.len() as u64 > MAX_FRAME_BYTES {
        return Err(RenderError::Frame);
    }
    if data.starts_with(b"\x89PNG\r\n\x1a\n") {
        return ImageSurface::create_from_png(&mut Cursor::new(data))
            .map_err(|_| RenderError::Frame);
    }
    let loader = gdk_pixbuf::PixbufLoader::new();
    loader.write(&data).map_err(|_| RenderError::Frame)?;
    loader.close().map_err(|_| RenderError::Frame)?;
    surface_of(&loader.pixbuf().ok_or(RenderError::Frame)?)
}

/// Copy 8-bit RGB(A) pixels into Cairo's premultiplied native-endian ARGB32.
fn surface_of(pixbuf: &gdk_pixbuf::Pixbuf) -> Result<ImageSurface, RenderError> {
    let (width, height) = (pixbuf.width(), pixbuf.height());
    if pixbuf.bits_per_sample() != 8 || width <= 0 || height <= 0 {
        return Err(RenderError::Frame);
    }
    let channels = pixbuf.n_channels() as usize;
    let alpha = pixbuf.has_alpha();
    let source_stride = pixbuf.rowstride() as usize;
    let bytes = pixbuf.read_pixel_bytes();
    let mut surface = ImageSurface::create(Format::ARgb32, width, height)?;
    let stride = surface.stride() as usize;
    {
        let mut target = surface.data().map_err(|_| RenderError::Frame)?;
        for y in 0..height as usize {
            for x in 0..width as usize {
                let s = y * source_stride + x * channels;
                let Some(pixel) = bytes.get(s..s + channels) else {
                    return Err(RenderError::Frame);
                };
                let a = if alpha { pixel[3] as u32 } else { 255 };
                let premultiply = |c: u8| ((c as u32 * a + 127) / 255) as u8;
                let value = (a << 24)
                    | ((premultiply(pixel[0]) as u32) << 16)
                    | ((premultiply(pixel[1]) as u32) << 8)
                    | premultiply(pixel[2]) as u32;
                let t = y * stride + x * 4;
                target[t..t + 4].copy_from_slice(&value.to_ne_bytes());
            }
        }
    }
    surface.mark_dirty();
    Ok(surface)
}

/// The grey thumbnail keyframe selection compares.
pub fn luma(surface: &mut ImageSurface) -> Vec<f32> {
    surface.flush();
    let (width, height, stride) = (
        surface.width() as usize,
        surface.height() as usize,
        surface.stride() as usize,
    );
    match surface.data() {
        Ok(data) => crate::keyframes::luma(&data, stride, width, height),
        Err(_) => vec![],
    }
}

pub fn png(surface: &ImageSurface) -> Result<Vec<u8>, RenderError> {
    let mut data = Vec::new();
    surface
        .write_to_png(&mut data)
        .map_err(|_| RenderError::Frame)?;
    Ok(data)
}

/// The part of the frame to keep, in logical screen pixels `[x0, y0, x1, y1]`.
pub fn crop(keyframe: &Keyframe, marks: &[Mark], frame: &Frame, screen: &Screen) -> [f32; 4] {
    let (w, h) = (screen.width.max(1) as f32, screen.height.max(1) as f32);
    let full = [0.0, 0.0, w, h];
    let shown: Vec<&Mark> = keyframe
        .marks
        .iter()
        .filter_map(|&i| marks.get(i))
        .filter(|mark| mark.screen == frame.screen)
        .collect();
    let Some(first) = shown.first() else {
        return frame
            .window
            .map(|[x0, y0, x1, y1]| clamp([x0 - 24.0, y0 - 24.0, x1 + 24.0, y1 + 24.0], w, h))
            .unwrap_or(full);
    };
    let mut b = first.region;
    for mark in &shown[1..] {
        b = [
            b[0].min(mark.region[0]),
            b[1].min(mark.region[1]),
            b[2].max(mark.region[2]),
            b[3].max(mark.region[3]),
        ];
    }
    // Keep enough of the screen around the ink to recognize where it is.
    let pad = (0.5 * (b[2] - b[0]).max(b[3] - b[1])).max(160.0);
    let mut c = [b[0] - pad, b[1] - pad, b[2] + pad, b[3] + pad];
    let grow = |c: &mut [f32; 4], axis: usize, size: f32| {
        let (lo, hi) = (axis, axis + 2);
        let missing = size - (c[hi] - c[lo]);
        if missing > 0.0 {
            c[lo] -= missing / 2.0;
            c[hi] += missing / 2.0;
        }
    };
    grow(&mut c, 0, w.min(900.0));
    grow(&mut c, 1, h.min(560.0));
    let (cw, ch) = (c[2] - c[0], c[3] - c[1]);
    if cw / ch < 1.2 {
        grow(&mut c, 0, (ch * 1.2).min(w));
    } else if cw / ch > 2.4 {
        grow(&mut c, 1, (cw / 2.4).min(h));
    }
    clamp(c, w, h)
}

/// Shift a box inside `0..w × 0..h`, shrinking it only when it is larger.
fn clamp(c: [f32; 4], w: f32, h: f32) -> [f32; 4] {
    let fit = |lo: f32, hi: f32, limit: f32| {
        let size = (hi - lo).min(limit);
        let lo = lo.clamp(0.0, limit - size);
        (lo, lo + size)
    };
    let (x0, x1) = fit(c[0], c[2], w);
    let (y0, y1) = fit(c[1], c[3], h);
    [x0, y0, x1, y1]
}

/// Render one keyframe from its frame image.
pub fn keyframe(
    source: &ImageSurface,
    keyframe: &Keyframe,
    marks: &[Mark],
    strokes: &[Stroke],
    frame: &Frame,
    screen: &Screen,
) -> Result<ImageSurface, RenderError> {
    let sx = source.width() as f64 / screen.width.max(1) as f64;
    let sy = source.height() as f64 / screen.height.max(1) as f64;
    let c = crop(keyframe, marks, frame, screen);
    let (px, py) = (c[0] as f64 * sx, c[1] as f64 * sy);
    let (pw, ph) = (
        ((c[2] - c[0]) as f64 * sx).max(1.0),
        ((c[3] - c[1]) as f64 * sy).max(1.0),
    );
    let fit = (MAX_EDGE / pw.max(ph)).min(1.0);
    let (ow, oh) = ((pw * fit).round().max(1.0), (ph * fit).round().max(1.0));
    // The badge sits in a strip above the frame, so it never hides what was marked.
    let strip = strip_height(oh);
    let output = ImageSurface::create(Format::Rgb24, ow as i32, (oh + strip) as i32)?;
    let cr = Context::new(&output)?;
    cr.set_source_rgb(0.12, 0.12, 0.12);
    cr.paint()?;
    badge(
        &cr,
        &format!("#{} {}", keyframe.index, format_clock(keyframe.t)),
        strip,
    )?;
    cr.translate(0.0, strip);
    cr.save()?;
    cr.rectangle(0.0, 0.0, ow, oh);
    cr.clip();
    cr.scale(fit, fit);
    cr.set_source_surface(source, -px, -py)?;
    cr.source().set_filter(cairo::Filter::Good);
    cr.paint()?;
    cr.restore()?;
    // Ink outside the crop stays out of the badge strip.
    cr.rectangle(0.0, 0.0, ow, oh);
    cr.clip();
    // Logical screen pixels to output pixels.
    let to = |x: f32, y: f32| ((x as f64 * sx - px) * fit, (y as f64 * sy - py) * fit);
    let unit = sx * fit;
    for mark in keyframe
        .marks
        .iter()
        .filter_map(|&i| marks.get(i))
        .filter(|mark| mark.screen == frame.screen)
    {
        if mark.shape == Shape::Pin {
            let (x, y) = to(mark.region[0], mark.region[1]);
            pin(&cr, x, y, unit, mark.pin.unwrap_or(1))?;
            continue;
        }
        for stroke in mark.strokes.iter().filter_map(|&i| strokes.get(i)) {
            ink(&cr, stroke, &to, unit)?;
        }
    }
    drop(cr);
    Ok(output)
}

fn ink(
    cr: &Context,
    stroke: &Stroke,
    to: &dyn Fn(f32, f32) -> (f64, f64),
    unit: f64,
) -> Result<(), RenderError> {
    let Some(first) = stroke.points.first() else {
        return Ok(());
    };
    cr.set_line_cap(cairo::LineCap::Round);
    cr.set_line_join(cairo::LineJoin::Round);
    let width = (4.0 * unit).max(3.0);
    for (rgba, line) in [
        ((1.0, 1.0, 1.0, 0.85), width + (2.5 * unit).max(2.0)),
        ((INK.0, INK.1, INK.2, 1.0), width),
    ] {
        let (x, y) = to(first.x, first.y);
        cr.move_to(x, y);
        for point in &stroke.points[1..] {
            let (x, y) = to(point.x, point.y);
            cr.line_to(x, y);
        }
        if stroke.points.len() == 1 {
            cr.line_to(x + 0.1, y);
        }
        cr.set_source_rgba(rgba.0, rgba.1, rgba.2, rgba.3);
        cr.set_line_width(line);
        cr.stroke()?;
    }
    Ok(())
}

fn pin(cr: &Context, x: f64, y: f64, unit: f64, number: u32) -> Result<(), RenderError> {
    let radius = (13.0 * unit).max(10.0);
    cr.arc(x, y, radius, 0.0, std::f64::consts::TAU);
    cr.set_source_rgb(INK.0, INK.1, INK.2);
    cr.fill_preserve()?;
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.set_line_width((2.0 * unit).max(1.5));
    cr.stroke()?;
    let layout = pangocairo::functions::create_layout(cr);
    let mut font = pango::FontDescription::from_string("Monospace Bold");
    font.set_absolute_size(radius * 1.1 * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    layout.set_text(&number.to_string());
    let (_, extent) = layout.pixel_extents();
    cr.move_to(
        x - extent.width() as f64 / 2.0 - extent.x() as f64,
        y - extent.height() as f64 / 2.0 - extent.y() as f64,
    );
    pangocairo::functions::show_layout(cr, &layout);
    Ok(())
}

fn strip_height(frame_height: f64) -> f64 {
    (frame_height * 0.05).clamp(30.0, 44.0).round()
}

/// `text` in a light rounded badge, vertically centred in a strip `strip` pixels high.
fn badge(cr: &Context, text: &str, strip: f64) -> Result<(), RenderError> {
    let size = (strip * 0.5).round();
    let layout = pangocairo::functions::create_layout(cr);
    let mut font = pango::FontDescription::from_string("Monospace Bold");
    font.set_absolute_size(size * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    let (_, extent) = layout.pixel_extents();
    let pad = size * 0.45;
    let (w, h) = (
        extent.width() as f64 + 2.0 * pad,
        extent.height() as f64 + pad,
    );
    let (x, y) = (size * 0.5, ((strip - h) / 2.0).max(0.0));
    rounded(cr, x, y, w, h, size * 0.3);
    cr.set_source_rgb(INK.0, INK.1, INK.2);
    cr.fill()?;
    cr.set_source_rgb(1.0, 1.0, 1.0);
    cr.move_to(
        x + pad - extent.x() as f64,
        y + pad * 0.5 - extent.y() as f64,
    );
    pangocairo::functions::show_layout(cr, &layout);
    Ok(())
}

fn rounded(cr: &Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::FRAC_PI_2;
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, 2.0 * FRAC_PI_2);
    cr.arc(x + r, y + r, r, 2.0 * FRAC_PI_2, 3.0 * FRAC_PI_2);
    cr.close_path();
}

/// All keyframes in time order on one image, at most `MAX_EDGE` wide.
pub fn contact_sheet(frames: &[ImageSurface]) -> Result<ImageSurface, RenderError> {
    let n = frames.len().max(1);
    let columns = match n {
        1 | 2 => n,
        3 | 4 => 2,
        5..=9 => 3,
        _ => 4,
    };
    let rows = n.div_ceil(columns);
    let gap = 12.0;
    let cell_w = ((MAX_EDGE - gap * (columns as f64 + 1.0)) / columns as f64).floor();
    let cell_h = (cell_w * 10.0 / 16.0).floor();
    let width = gap + columns as f64 * (cell_w + gap);
    let height = gap + rows as f64 * (cell_h + gap);
    let sheet = ImageSurface::create(Format::Rgb24, width as i32, height as i32)?;
    let cr = Context::new(&sheet)?;
    cr.set_source_rgb(0.12, 0.12, 0.12);
    cr.paint()?;
    for (i, frame) in frames.iter().enumerate() {
        let (col, row) = ((i % columns) as f64, (i / columns) as f64);
        let (x, y) = (gap + col * (cell_w + gap), gap + row * (cell_h + gap));
        let (fw, fh) = (frame.width() as f64, frame.height() as f64);
        let scale = (cell_w / fw).min(cell_h / fh);
        let (dw, dh) = (fw * scale, fh * scale);
        cr.save()?;
        cr.translate(x + (cell_w - dw) / 2.0, y + (cell_h - dh) / 2.0);
        cr.scale(scale, scale);
        cr.set_source_surface(frame, 0.0, 0.0)?;
        cr.source().set_filter(cairo::Filter::Good);
        cr.paint()?;
        cr.restore()?;
    }
    drop(cr);
    Ok(sheet)
}
