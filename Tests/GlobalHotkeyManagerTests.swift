import Testing
import CoreGraphics
import Foundation
@testable import VoiceScribeMac

@Suite("Global Hotkey Manager")
struct GlobalHotkeyManagerTests {
    let manager = GlobalHotkeyManager(modifierOnlyActivationDelay: 0)

    private func makeKeyEvent(
        keyCode: UInt16,
        flags: CGEventFlags,
        keyDown: Bool = true,
        timestamp: TimeInterval? = nil
    ) -> CGEvent? {
        let event = CGEvent(keyboardEventSource: nil, virtualKey: keyCode, keyDown: keyDown)
        event?.flags = flags
        if let timestamp {
            event?.timestamp = UInt64(timestamp * 1_000_000_000)
        }
        return event
    }

    // MARK: - Default Hotkey

    @Test("Default hotkey Right Command is recognized")
    func defaultHotkeyRecognized() throws {
        let event = try #require(makeKeyEvent(keyCode: 54, flags: .maskCommand))
        #expect(manager.isHotkey(event))
    }

    @Test("Left Command does not trigger the Right Command hotkey")
    func leftCommandNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 55, flags: .maskCommand))
        #expect(!manager.isHotkey(event))
    }

    @Test("Right Command with another modifier is not the hotkey")
    func rightCommandWithExtraModifierNotHotkey() throws {
        let event = try #require(makeKeyEvent(
            keyCode: 54,
            flags: [.maskCommand, .maskShift]
        ))
        #expect(!manager.isHotkey(event))
    }

    @Test("Plain V without modifiers is not hotkey")
    func plainVNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 9, flags: []))
        #expect(!manager.isHotkey(event))
    }

    @Test("Plain V key events pass through")
    func plainVKeyEventsPassThrough() throws {
        let keyDown = try #require(makeKeyEvent(keyCode: 9, flags: []))
        let keyUp = try #require(CGEvent(
            keyboardEventSource: nil,
            virtualKey: 9,
            keyDown: false
        ))

        #expect(!manager.handleGestureEvent(type: .keyDown, event: keyDown))
        #expect(!manager.handleGestureEvent(type: .keyUp, event: keyUp))
    }

    @Test("Chord key-up is consumed after modifiers are released")
    func hotkeyKeyUpConsumedAfterModifierRelease() throws {
        manager.configure(keyCode: 9, modifiers: [.maskControl, .maskShift])
        let keyDown = try #require(makeKeyEvent(
            keyCode: 9,
            flags: [.maskControl, .maskShift]
        ))
        let keyUp = try #require(CGEvent(
            keyboardEventSource: nil,
            virtualKey: 9,
            keyDown: false
        ))

        #expect(manager.handleGestureEvent(type: .keyDown, event: keyDown))
        #expect(manager.handleGestureEvent(type: .keyUp, event: keyUp))
    }

    @Test("Right Command controls capture while modifier events pass through")
    func modifierOnlyHotkeyPassesThrough() throws {
        var starts = 0
        var stops = 0
        manager.onStartCapture = { starts += 1 }
        manager.onStopCapture = { stops += 1 }

        let keyDown = try #require(makeKeyEvent(
            keyCode: 54,
            flags: .maskCommand,
            timestamp: 1
        ))
        let keyUp = try #require(makeKeyEvent(
            keyCode: 54,
            flags: [],
            keyDown: false,
            timestamp: 2
        ))

        #expect(!manager.handleGestureEvent(type: .flagsChanged, event: keyDown))
        #expect(starts == 1)
        #expect(!manager.handleGestureEvent(type: .flagsChanged, event: keyUp))
        #expect(stops == 1)
    }

    @Test("Right Command shortcut chords cancel capture and pass through")
    func modifierOnlyHotkeyInterruptedByKey() throws {
        let manager = GlobalHotkeyManager(modifierOnlyActivationDelay: 0.15)
        var starts = 0
        var cancellations = 0
        manager.onStartCapture = { starts += 1 }
        manager.onCancelCapture = { cancellations += 1 }

        let commandDown = try #require(makeKeyEvent(
            keyCode: 54,
            flags: .maskCommand,
            timestamp: 1
        ))
        let copyKeyDown = try #require(makeKeyEvent(
            keyCode: 8,
            flags: .maskCommand,
            timestamp: 1.1
        ))
        let commandUp = try #require(makeKeyEvent(
            keyCode: 54,
            flags: [],
            keyDown: false,
            timestamp: 1.2
        ))

        #expect(!manager.handleGestureEvent(type: .flagsChanged, event: commandDown))
        #expect(starts == 0)
        #expect(!manager.handleGestureEvent(type: .keyDown, event: copyKeyDown))
        #expect(starts == 0)
        #expect(cancellations == 0)
        #expect(!manager.handleGestureEvent(type: .flagsChanged, event: commandUp))
    }

    @Test("Pointer input cancels a modifier-only capture")
    func modifierOnlyHotkeyInterruptedByPointer() throws {
        var cancellations = 0
        manager.onCancelCapture = { cancellations += 1 }

        let commandDown = try #require(makeKeyEvent(
            keyCode: 54,
            flags: .maskCommand,
            timestamp: 1
        ))
        #expect(!manager.handleGestureEvent(type: .flagsChanged, event: commandDown))
        #expect(manager.cancelModifierOnlyGesture())
        #expect(cancellations == 1)
        #expect(!manager.cancelModifierOnlyGesture())
    }

    @Test("Ctrl+V without Shift is not hotkey (missing modifier)")
    func ctrlVNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 9, flags: .maskControl))
        #expect(!manager.isHotkey(event))
    }

    @Test("Shift+V without Ctrl is not hotkey (missing modifier)")
    func shiftVNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 9, flags: .maskShift))
        #expect(!manager.isHotkey(event))
    }

    @Test("Cmd+Ctrl+Shift+V is not hotkey (extra modifier)")
    func cmdCtrlShiftVNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 9, flags: [.maskControl, .maskShift, .maskCommand]))
        #expect(!manager.isHotkey(event))
    }

    @Test("Option+Ctrl+Shift+V is not hotkey (extra modifier)")
    func optCtrlShiftVNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 9, flags: [.maskControl, .maskShift, .maskAlternate]))
        #expect(!manager.isHotkey(event))
    }

    @Test("Ctrl+Shift+S (different key) is not hotkey")
    func ctrlShiftSNotHotkey() throws {
        let event = try #require(makeKeyEvent(keyCode: 1, flags: [.maskControl, .maskShift]))
        #expect(!manager.isHotkey(event))
    }

    // MARK: - Configurable Hotkey

    @Test("Configure changes the recognized hotkey")
    func configureChangesHotkey() throws {
        let mgr = GlobalHotkeyManager()
        // Configure to Option+S (like old default)
        mgr.configure(keyCode: 1, modifiers: .maskAlternate)

        let event = try #require(makeKeyEvent(keyCode: 1, flags: .maskAlternate))
        #expect(mgr.isHotkey(event))

        // Old default should no longer match
        let oldDefault = try #require(makeKeyEvent(keyCode: 9, flags: [.maskControl, .maskShift]))
        #expect(!mgr.isHotkey(oldDefault))
    }

    @Test("Configure to Cmd+Shift+R")
    func configureCmdShiftR() throws {
        let mgr = GlobalHotkeyManager()
        mgr.configure(keyCode: 15, modifiers: CGEventFlags([.maskCommand, .maskShift]))

        let matching = try #require(makeKeyEvent(keyCode: 15, flags: [.maskCommand, .maskShift]))
        #expect(mgr.isHotkey(matching))

        // Extra modifier should fail
        let extra = try #require(makeKeyEvent(keyCode: 15, flags: [.maskCommand, .maskShift, .maskControl]))
        #expect(!mgr.isHotkey(extra))

        // Missing modifier should fail
        let missing = try #require(makeKeyEvent(keyCode: 15, flags: .maskCommand))
        #expect(!mgr.isHotkey(missing))
    }

    @Test("Configure to single modifier Ctrl+Space")
    func configureCtrlSpace() throws {
        let mgr = GlobalHotkeyManager()
        mgr.configure(keyCode: 49, modifiers: .maskControl)

        let matching = try #require(makeKeyEvent(keyCode: 49, flags: .maskControl))
        #expect(mgr.isHotkey(matching))

        // Option not part of combo — should reject
        let withOption = try #require(makeKeyEvent(keyCode: 49, flags: [.maskControl, .maskAlternate]))
        #expect(!mgr.isHotkey(withOption))
    }

    // MARK: - Display String

    @Test("Default display string distinguishes Right Command")
    func defaultDisplayString() {
        let mgr = GlobalHotkeyManager()
        #expect(mgr.displayString == "Right \u{2318}")
    }

    @Test("Display string updates after configure")
    func displayStringUpdates() {
        let mgr = GlobalHotkeyManager()
        mgr.configure(keyCode: 1, modifiers: .maskAlternate)
        #expect(mgr.displayString == "\u{2325}S")
    }

    @Test("Display string with multiple modifiers")
    func displayStringMultipleModifiers() {
        let mgr = GlobalHotkeyManager()
        mgr.configure(keyCode: 15, modifiers: CGEventFlags([.maskCommand, .maskShift]))
        // Order: Ctrl, Option, Shift, Command
        #expect(mgr.displayString == "\u{21E7}\u{2318}R")
    }
}
