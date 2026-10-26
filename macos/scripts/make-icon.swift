// Compose the macOS app icon from the brand's glossy mark: the red jelly floating on a pale,
// glass-like tile drawn to Apple's macOS icon grid (an 824-point rounded square in 1024).
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
    context.setShadow(offset: CGSize(width: 0, height: -10), blur: 28, color: CGColor(gray: 0, alpha: 0.32))
    context.addPath(shape)
    context.setFillColor(CGColor(gray: 0.94, alpha: 1))
    context.fillPath()
    context.restoreGState()
    // Frosted glass: a cool-to-warm pale gradient with a bright upper edge.
    context.saveGState()
    context.addPath(shape)
    context.clip()
    let glass = CGGradient(colorsSpace: space, colors: [
        CGColor(red: 0.99, green: 0.97, blue: 0.97, alpha: 1),
        CGColor(red: 0.93, green: 0.93, blue: 0.95, alpha: 1),
        CGColor(red: 0.86, green: 0.87, blue: 0.90, alpha: 1),
    ] as CFArray, locations: [0, 0.55, 1])!
    context.drawLinearGradient(glass, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 100), options: [])
    // The jelly's red light diffusing into the glass.
    let glow = CGGradient(colorsSpace: space, colors: [
        CGColor(red: 1, green: 0.25, blue: 0.28, alpha: 0.30),
        CGColor(red: 1, green: 0.25, blue: 0.28, alpha: 0),
    ] as CFArray, locations: [0, 1])!
    context.drawRadialGradient(glow, startCenter: CGPoint(x: 512, y: 470), startRadius: 0,
                               endCenter: CGPoint(x: 512, y: 470), endRadius: 430, options: [])
    // Specular sheen across the top.
    let sheen = CGGradient(colorsSpace: space, colors: [
        CGColor(gray: 1, alpha: 0.75), CGColor(gray: 1, alpha: 0),
    ] as CFArray, locations: [0, 1])!
    context.drawLinearGradient(sheen, start: CGPoint(x: 512, y: 924), end: CGPoint(x: 512, y: 760), options: [])
    context.restoreGState()
    // Inner rim.
    context.addPath(shape)
    context.setStrokeColor(CGColor(gray: 1, alpha: 0.7))
    context.setLineWidth(3)
    context.strokePath()
    // The glossy mark, with a soft contact shadow.
    context.saveGState()
    context.setShadow(offset: CGSize(width: 0, height: -22), blur: 40, color: CGColor(red: 0.55, green: 0.02, blue: 0.06, alpha: 0.45))
    context.draw(mark, in: CGRect(x: 227, y: 232, width: 570, height: 570))
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
