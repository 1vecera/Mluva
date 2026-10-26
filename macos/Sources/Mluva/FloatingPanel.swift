import AppKit
import SwiftUI

/// A borderless, transparent panel above other windows on every Space. The recording pill
/// never takes focus; the wheel becomes key for its letter keys without activating Mluva.
final class FloatingPanel: NSPanel {
    private let keyable: Bool
    var onEscape: (() -> Void)?
    /// Keys typed while the panel is key; return true when handled.
    var onKey: ((NSEvent) -> Bool)?

    init<Content: View>(keyable: Bool, @ViewBuilder content: @escaping () -> Content?) {
        self.keyable = keyable
        super.init(
            contentRect: NSRect(x: 0, y: 0, width: 400, height: 200),
            styleMask: [.borderless, .nonactivatingPanel, .fullSizeContentView],
            backing: .buffered, defer: true)
        isOpaque = false
        backgroundColor = .clear
        hasShadow = false
        level = keyable ? .modalPanel : .statusBar
        collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        isMovableByWindowBackground = !keyable
        hidesOnDeactivate = false
        becomesKeyOnlyIfNeeded = !keyable
        isReleasedWhenClosed = false
        animationBehavior = .none
        // Panels never become main, but their controls should look live, not inactive.
        let host = NSHostingView(rootView: content().environment(\.controlActiveState, .key))
        host.sizingOptions = []
        contentView = host
    }

    override var canBecomeKey: Bool { keyable }
    override var canBecomeMain: Bool { false }

    override func keyDown(with event: NSEvent) {
        if onKey?(event) != true { super.keyDown(with: event) }
    }

    override func cancelOperation(_ sender: Any?) {
        onEscape?()
    }

    func fillScreen(_ screen: NSScreen?) {
        guard let screen else { return }
        setFrame(screen.frame, display: false)
    }

    func placeBottomCenter(of screen: NSScreen?, size: CGSize) {
        guard let screen else { return }
        let area = screen.visibleFrame
        setFrame(NSRect(x: area.midX - size.width / 2, y: area.minY + 28, width: size.width, height: size.height), display: false)
    }

    func present() {
        alphaValue = 0
        if keyable { makeKeyAndOrderFront(nil) } else { orderFrontRegardless() }
        NSAnimationContext.runAnimationGroup { context in
            context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.18
            animator().alphaValue = 1
        }
    }

    func dismiss() {
        NSAnimationContext.runAnimationGroup({ context in
            context.duration = NSWorkspace.shared.accessibilityDisplayShouldReduceMotion ? 0 : 0.22
            animator().alphaValue = 0
        }, completionHandler: { [weak self] in
            MainActor.assumeIsolated { self?.orderOut(nil) }
        })
    }
}
