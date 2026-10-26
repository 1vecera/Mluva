import AppKit
import CoreGraphics
import ImageIO
import ScreenCaptureKit
import UniformTypeIdentifiers

/// One display as annotation names and measures it: logical pixels, origin top left of the
/// primary display, like the shared model's layout coordinates.
struct AnnotatedScreen: Sendable {
    let name: String
    let display: CGDirectDisplayID
    /// AppKit's frame, origin bottom left.
    let frame: CGRect
    let scale: Double
    let primaryHeight: Double

    var model: Annotation.Screen {
        Annotation.Screen(
            name: name, x: Int(frame.minX), y: Int(primaryHeight - frame.maxY),
            width: Int(frame.width), height: Int(frame.height), scale: scale)
    }

    /// AppKit's global point in this screen's top-left logical pixels.
    func local(_ point: CGPoint) -> CGPoint {
        CGPoint(x: point.x - frame.minX, y: frame.maxY - point.y)
    }

    @MainActor
    static func all() -> [AnnotatedScreen] {
        let primary = NSScreen.screens.first?.frame.height ?? 0
        var seen: [String: Int] = [:]
        return NSScreen.screens.compactMap { screen in
            guard let number = screen.deviceDescription[NSDeviceDescriptionKey("NSScreenNumber")] as? NSNumber else { return nil }
            var name = screen.localizedName
            seen[name, default: 0] += 1
            if let count = seen[name], count > 1 { name += " \(count)" }
            return AnnotatedScreen(
                name: name, display: number.uint32Value, frame: screen.frame,
                scale: screen.backingScaleFactor, primaryHeight: primary)
        }
    }
}

/// A frame kept in memory: compressed, never written to disk.
struct HeldFrame: Sendable {
    let data: Data
    let luma: [Float]
}

/// Clean screen captures through ScreenCaptureKit, leaving out Mluva's own windows, so ink,
/// the pill and the wheel never appear in a keyframe.
enum ScreenFrames {
    static let thumbWidth = 64
    static let thumbHeight = 36
    /// Long edge a raw frame is captured at; keyframes are at most 1600 pixels.
    static let maxEdge = 2560.0

    static var permitted: Bool { CGPreflightScreenCaptureAccess() }

    /// The display without this process's windows, at up to `maxEdge` pixels.
    static func capture(_ screen: AnnotatedScreen) async throws -> CGImage {
        let content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true)
        guard let display = content.displays.first(where: { $0.displayID == screen.display }) else {
            throw CocoaError(.fileNoSuchFile)
        }
        let own = content.windows.filter { $0.owningApplication?.processID == getpid() }
        let filter = SCContentFilter(display: display, excludingWindows: own)
        let configuration = SCStreamConfiguration()
        let pixels = min(screen.scale, maxEdge / max(screen.frame.width, screen.frame.height))
        configuration.width = Int(screen.frame.width * pixels)
        configuration.height = Int(screen.frame.height * pixels)
        configuration.showsCursor = false
        configuration.captureResolution = .best
        return try await SCScreenshotManager.captureImage(contentFilter: filter, configuration: configuration)
    }

    /// Compress for holding, and measure the grey thumbnail the planner compares.
    static func hold(_ image: CGImage) -> HeldFrame? {
        let data = NSMutableData()
        guard let destination = CGImageDestinationCreateWithData(data, UTType.jpeg.identifier as CFString, 1, nil) else { return nil }
        CGImageDestinationAddImage(destination, image, [kCGImageDestinationLossyCompressionQuality: 0.9] as CFDictionary)
        guard CGImageDestinationFinalize(destination) else { return nil }
        return HeldFrame(data: data as Data, luma: luma(image))
    }

    static func image(_ held: HeldFrame) -> CGImage? {
        guard let source = CGImageSourceCreateWithData(held.data as CFData, nil) else { return nil }
        return CGImageSourceCreateImageAtIndex(source, 0, nil)
    }

    /// A 64×36 grey thumbnail with the shared planner's luma weights.
    static func luma(_ image: CGImage) -> [Float] {
        var pixels = [UInt8](repeating: 0, count: thumbWidth * thumbHeight * 4)
        let drawn = pixels.withUnsafeMutableBytes { buffer -> Bool in
            guard let context = CGContext(
                data: buffer.baseAddress, width: thumbWidth, height: thumbHeight, bitsPerComponent: 8,
                bytesPerRow: thumbWidth * 4, space: CGColorSpace(name: CGColorSpace.sRGB)!,
                bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue)
            else { return false }
            context.interpolationQuality = .medium
            context.draw(image, in: CGRect(x: 0, y: 0, width: thumbWidth, height: thumbHeight))
            return true
        }
        guard drawn else { return [Float](repeating: 0, count: thumbWidth * thumbHeight) }
        return (0..<(thumbWidth * thumbHeight)).map { i in
            (0.2126 * Float(pixels[i * 4]) + 0.7152 * Float(pixels[i * 4 + 1]) + 0.0722 * Float(pixels[i * 4 + 2])) / 255
        }
    }

    /// Other applications' visible windows on `screen`, frontmost first, without titles.
    static func windows(on screen: AnnotatedScreen) -> [Annotation.WindowBox] {
        guard let list = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID) as? [[String: Any]] else {
            return []
        }
        let front = NSWorkspace.shared.frontmostApplication?.processIdentifier
        let origin = CGPoint(x: screen.frame.minX, y: screen.primaryHeight - screen.frame.maxY)
        let bounds = CGRect(origin: .zero, size: screen.frame.size)
        var activeTaken = false
        return list.compactMap { info in
            guard (info[kCGWindowLayer as String] as? Int) == 0,
                  let pid = info[kCGWindowOwnerPID as String] as? pid_t, pid != getpid(),
                  let dictionary = info[kCGWindowBounds as String] as? NSDictionary,
                  let global = CGRect(dictionaryRepresentation: dictionary)
            else { return nil }
            let rect = global.offsetBy(dx: -origin.x, dy: -origin.y).intersection(bounds)
            guard !rect.isNull, rect.width >= 40, rect.height >= 40 else { return nil }
            let active = !activeTaken && pid == front
            if active { activeTaken = true }
            return Annotation.WindowBox(rect: [rect.minX, rect.minY, rect.maxX, rect.maxY], active: active)
        }
    }
}
