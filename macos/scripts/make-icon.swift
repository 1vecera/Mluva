// Compose the macOS app icon from the brand's glossy mark: the red jelly inside a hairline
// protractor ring on a graphite tile drawn to Apple's macOS icon grid (824 points in 1024).
// Usage: swift make-icon.swift <mluva-mark-1024.png> <output.iconset>
import AppKit
import CoreGraphics

let arguments = CommandLine.arguments
guard arguments.count == 3, let mark = NSImage(contentsOfFile: arguments[1])?.cgImage(forProposedRect: nil, context: nil, hints: nil) else {
    FileHandle.standardError.write(Data("usage: make-icon.swift <mark.png> <out.iconset>\n".utf8))
    exit(2)
}
let output = URL(fileURLWithPath: arguments[2])
try FileManager.default.createDirectory(at: output, withIntermediateDirectories: true)

func render(_ size: Int) -> CGImage {
    let scale = CGFloat(size) / 1024
    let space = CGColorSpace(name: CGColorSpace.displayP3)!
    let context = CGContext(data: nil, width: size, height: size, bitsPerComponent: 8, bytesPerRow: 0,
                            space: space, bitmapInfo: CGImageAlphaInfo.premultipliedLast.rawValue)!
    context.scaleBy(x: scale, y: scale)
    let tile = CGRect(x: 100, y: 100, width: 824, height: 824)
    let shape = CGPath(roundedRect: tile, cornerWidth: 185, cornerHeight: 185, transform: nil)
    // Grid shadow under the tile.
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: CGColor(gray: 0, alpha: 0.38))
    context.addPath(shape)
    context.setFillColor(CGColor(gray: 0.04, alpha: 1))
    context.fillPath()
    context.restoreGState()
    // Graphite glass: near black, lit faintly from the top, as the brand puts the mark on black.
    context.saveGState()
    context.addPath(shape)
    context.clip()
    let glass = CGGradient(colorsSpace: space, colors: [
        CGColor(red: 0.16, green: 0.165, blue: 0.18, alpha: 1),
        CGColor(red: 0.07, green: 0.075, blue: 0.085, alpha: 1),
        CGColor(red: 0.03, green: 0.03, blue: 0.035, alpha: 1),
    ] as CFArray, locations: [0, 0.5, 1])!
    context.drawLinearGradient(glass, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    // The drafting ring: a protractor of hairline ticks around the mark, the red arc from twelve
    // o'clock like the session wheel's selection.
    let center = CGPoint(x: 512, y: 512)
    let radius: CGFloat = 330
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.22))
    context.setLineWidth(3)
    context.strokeEllipse(in: CGRect(x: center.x - radius, y: center.y - radius, width: radius * 2, height: radius * 2))
    for tick in 0..<72 {
        let angle = CGFloat(tick) * 5 * .pi / 180
        let long = tick % 6 == 0
        let inner = radius + 6, outer = radius + (long ? 40 : 20)
        context.move(to: CGPoint(x: center.x + inner * sin(angle), y: center.y + inner * cos(angle)))
        context.addLine(to: CGPoint(x: center.x + outer * sin(angle), y: center.y + outer * cos(angle)))
    }
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.30))
    context.setLineWidth(3)
    context.strokePath()
    context.addArc(center: center, radius: radius, startAngle: .pi / 2, endAngle: .pi / 2 - .pi * 2 / 3, clockwise: true)
    context.setStrokeColor(CGColor(red: 1, green: 0.25, blue: 0.28, alpha: 1))
    context.setLineWidth(9)
    context.setLineCap(.round)
    context.strokePath()
    context.restoreGState()
    // Inner rim.
    context.addPath(shape)
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.14))
    context.setLineWidth(3)
    context.strokePath()
    // The glossy mark, with a soft contact shadow.
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -22), blur: 40, color: CGColor(red: 0.55, green: 0.02, blue: 0.06, alpha: 0.45))
    context.draw(mark, in: CGRect(x: 302, y: 307, width: 420, height: 420))
    context.restoreGState()
    return context.makeImage()!
}

for (points, scales) in [(16, [1, 2]), (32, [1, 2]), (128, [1, 2]), (256, [1, 2]), (512, [1, 2])] {
    for scale in scales {
        let image = render(points * scale)
        let name = scale == 1 ? "icon_\(points)x\(points).png" : "icon_\(points)x\(points)@2x.png"
        let destination = CGImageDestinationCreateWithURL(output.appending(path: name) as CFURL, "public.png" as CFString, 1, nil)!
        CGImageDestinationAddImage(destination, image, nil)
        CGImageDestinationFinalize(destination)
    }
}
