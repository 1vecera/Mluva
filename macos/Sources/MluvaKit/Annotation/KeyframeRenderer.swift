import AppKit
import CoreGraphics
import ImageIO
import UniformTypeIdentifiers

/// Draws keyframes the way the Linux renderer does: the planner's crop, the ink and pins, and a
/// `#N M:SS` badge in a strip above the frame, then a contact sheet of them all.
public enum KeyframeRenderer {
    /// Long edge of a rendered keyframe and of the contact sheet, in pixels.
    public static let maxEdge = 1600.0
    static let ink = CGColor(srgbRed: 1, green: 0.231, blue: 0.188, alpha: 1)
    static let halo = CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 0.85)
    static let background = CGColor(srgbRed: 0.12, green: 0.12, blue: 0.12, alpha: 1)

    public enum Failure: Error { case context, encode }

    /// One keyframe from its source frame. `crop` is `[x0, y0, x1, y1]` in logical pixels.
    public static func keyframe(
        source: CGImage, keyframe: Annotation.Keyframe, crop: [Double], marks: [Annotation.Mark],
        strokes: [Annotation.Stroke], frame: Annotation.Frame, screen: Annotation.Screen
    ) throws -> CGImage {
        let sx = Double(source.width) / Double(max(screen.width, 1))
        let sy = Double(source.height) / Double(max(screen.height, 1))
        let (px, py) = (crop[0] * sx, crop[1] * sy)
        let pw = max((crop[2] - crop[0]) * sx, 1)
        let ph = max((crop[3] - crop[1]) * sy, 1)
        let fit = min(maxEdge / max(pw, ph), 1)
        let (ow, oh) = (max((pw * fit).rounded(), 1), max((ph * fit).rounded(), 1))
        // The badge sits in a strip above the frame, so it never hides what was marked.
        let strip = min(max(oh * 0.05, 30), 44).rounded()
        let context = try canvas(width: Int(ow), height: Int(oh + strip))
        context.setFillColor(background)
        context.fill(CGRect(x: 0, y: 0, width: ow, height: oh + strip))
        badge(context, "#\(keyframe.index) \(Annotation.clock(keyframe.t))", strip: strip)
        context.translateBy(x: 0, y: strip)
        context.saveGState()
        context.clip(to: CGRect(x: 0, y: 0, width: ow, height: oh))
        context.interpolationQuality = .high
        // The canvas is flipped; images draw upright through a local flip.
        let drawn = CGRect(x: -px * fit, y: -py * fit, width: Double(source.width) * fit, height: Double(source.height) * fit)
        context.translateBy(x: 0, y: drawn.minY * 2 + drawn.height)
        context.scaleBy(x: 1, y: -1)
        context.draw(source, in: drawn)
        context.restoreGState()
        context.clip(to: CGRect(x: 0, y: 0, width: ow, height: oh))
        let to = { (x: Double, y: Double) in CGPoint(x: (x * sx - px) * fit, y: (y * sy - py) * fit) }
        let unit = sx * fit
        for mark in keyframe.marks.compactMap({ marks.indices.contains($0) ? marks[$0] : nil }) where mark.screen == frame.screen {
            if mark.shape == "pin" {
                pin(context, at: to(mark.region[0], mark.region[1]), unit: unit, number: mark.pin ?? 1)
                continue
            }
            for stroke in mark.strokes.compactMap({ strokes.indices.contains($0) ? strokes[$0] : nil }) {
                line(context, stroke.points.map { to($0.x, $0.y) }, unit: unit)
            }
        }
        guard let image = context.makeImage() else { throw Failure.context }
        return image
    }

    /// All keyframes in time order on one image, at most `maxEdge` wide.
    public static func contactSheet(_ frames: [CGImage]) throws -> CGImage {
        let n = max(frames.count, 1)
        let columns = switch n {
        case 1, 2: n
        case 3, 4: 2
        case 5...9: 3
        default: 4
        }
        let rows = (n + columns - 1) / columns
        let gap = 12.0
        let cellW = ((maxEdge - gap * Double(columns + 1)) / Double(columns)).rounded(.down)
        let cellH = (cellW * 10 / 16).rounded(.down)
        let width = gap + Double(columns) * (cellW + gap)
        let height = gap + Double(rows) * (cellH + gap)
        let context = try canvas(width: Int(width), height: Int(height))
        context.setFillColor(background)
        context.fill(CGRect(x: 0, y: 0, width: width, height: height))
        context.interpolationQuality = .high
        for (i, frame) in frames.enumerated() {
            let (col, row) = (Double(i % columns), Double(i / columns))
            let (x, y) = (gap + col * (cellW + gap), gap + row * (cellH + gap))
            let (fw, fh) = (Double(frame.width), Double(frame.height))
            let scale = min(cellW / fw, cellH / fh)
            let rect = CGRect(x: x + (cellW - fw * scale) / 2, y: y + (cellH - fh * scale) / 2, width: fw * scale, height: fh * scale)
            context.saveGState()
            context.translateBy(x: 0, y: rect.minY * 2 + rect.height)
            context.scaleBy(x: 1, y: -1)
            context.draw(frame, in: rect)
            context.restoreGState()
        }
        guard let image = context.makeImage() else { throw Failure.context }
        return image
    }

    /// Write an owner-only PNG, replacing nothing that is not a regular file.
    public static func writePNG(_ image: CGImage, to url: URL) throws {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.png.identifier as CFString, 1, nil) else { throw Failure.encode }
        CGImageDestinationAddImage(destination, image, nil)
        guard CGImageDestinationFinalize(destination) else { throw Failure.encode }
        guard FileManager.default.createFile(atPath: url.path, contents: data as Data, attributes: [.posixPermissions: 0o600]) else {
            throw CocoaError(.fileWriteUnknown)
        }
    }

    /// An sRGB canvas with a top-left origin, like the screen coordinates it draws.
    static func canvas(width: Int, height: Int) throws -> CGContext {
        guard let context = CGContext(
            data: nil, width: width, height: height, bitsPerComponent: 8, bytesPerRow: 0,
            space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
        else { throw Failure.context }
        context.translateBy(x: 0, y: Double(height))
        context.scaleBy(x: 1, y: -1)
        return context
    }

    static func line(_ context: CGContext, _ points: [CGPoint], unit: Double) {
        guard let first = points.first else { return }
        context.setLineCap(.round)
        context.setLineJoin(.round)
        let width = max(4 * unit, 3)
        for (color, lineWidth) in [(halo, width + max(2.5 * unit, 2)), (ink, width)] {
            context.beginPath()
            context.move(to: first)
            for point in points.dropFirst() { context.addLine(to: point) }
            if points.count == 1 { context.addLine(to: CGPoint(x: first.x + 0.1, y: first.y)) }
            context.setStrokeColor(color)
            context.setLineWidth(lineWidth)
            context.strokePath()
        }
    }

    static func pin(_ context: CGContext, at center: CGPoint, unit: Double, number: UInt32) {
        let radius = max(13 * unit, 10)
        let circle = CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2)
        context.setFillColor(ink)
        context.fillEllipse(in: circle)
        context.setStrokeColor(.white)
        context.setLineWidth(max(2 * unit, 1.5))
        context.strokeEllipse(in: circle)
        text(context, String(number), size: radius * 1.1, centeredAt: center)
    }

    /// `label` in a red rounded badge, vertically centred in a strip `strip` pixels high.
    static func badge(_ context: CGContext, _ label: String, strip: Double) {
        let size = (strip * 0.5).rounded()
        let bounds = CTLineGetBoundsWithOptions(typeset(label, size: size), .useGlyphPathBounds)
        let pad = size * 0.45
        let (w, h) = (bounds.width + 2 * pad, size + pad)
        let rect = CGRect(x: size * 0.5, y: max((strip - h) / 2, 0), width: w, height: h)
        context.addPath(CGPath(roundedRect: rect, cornerWidth: size * 0.3, cornerHeight: size * 0.3, transform: nil))
        context.setFillColor(ink)
        context.fillPath()
        text(context, label, size: size, centeredAt: CGPoint(x: rect.midX, y: rect.midY))
    }

    static func typeset(_ string: String, size: Double) -> CTLine {
        let font = NSFont.monospacedSystemFont(ofSize: size, weight: .bold)
        let attributed = NSAttributedString(string: string, attributes: [.font: font, .foregroundColor: NSColor.white])
        return CTLineCreateWithAttributedString(attributed)
    }

    /// White text centred on `center` in the flipped canvas.
    static func text(_ context: CGContext, _ string: String, size: Double, centeredAt center: CGPoint) {
        let line = typeset(string, size: size)
        // Glyph bounds, independent of where the context's text position was left.
        let bounds = CTLineGetBoundsWithOptions(line, .useGlyphPathBounds)
        context.saveGState()
        context.textMatrix = .identity
        // Text draws upright in a locally unflipped space around its baseline.
        context.translateBy(x: center.x - bounds.midX, y: center.y + bounds.midY)
        context.scaleBy(x: 1, y: -1)
        context.textPosition = .zero
        CTLineDraw(line, context)
        context.restoreGState()
    }
}
