import Testing
@testable import VoiceScribeMac

@Suite("Hotkey gesture interpretation")
struct HotkeyGestureInterpreterTests {
    @Test("Press and release produces hold-to-talk actions")
    func holdToTalk() {
        var interpreter = HotkeyGestureInterpreter()

        #expect(interpreter.keyDown(at: 0, isRepeat: false) == .beginHold)
        #expect(interpreter.keyUp(at: 1) == .endHoldNow)
    }

    @Test("Auto-repeat never toggles recording")
    func ignoresAutoRepeat() {
        var interpreter = HotkeyGestureInterpreter()

        #expect(interpreter.keyDown(at: 0, isRepeat: false) == .beginHold)
        #expect(interpreter.keyDown(at: 0.4, isRepeat: true) == nil)
        #expect(interpreter.keyUp(at: 1) == .endHoldNow)
    }

    @Test("Double tap converts the active capture to hands-free")
    func doubleTapStartsHandsFree() {
        var interpreter = HotkeyGestureInterpreter(
            shortTapMaximumDuration: 0.2,
            doubleTapWindow: 0.3
        )

        #expect(interpreter.keyDown(at: 0, isRepeat: false) == .beginHold)
        #expect(interpreter.keyUp(at: 0.1) == .scheduleEndHold)
        #expect(interpreter.keyDown(at: 0.2, isRepeat: false) == .continueHandsFree)
        #expect(interpreter.keyUp(at: 0.25) == nil)
        #expect(interpreter.keyDown(at: 1, isRepeat: false) == .endHandsFree)
    }

    @Test("An unpaired short tap ends when its double-tap window expires")
    func shortTapExpires() {
        var interpreter = HotkeyGestureInterpreter()

        #expect(interpreter.keyDown(at: 0, isRepeat: false) == .beginHold)
        #expect(interpreter.keyUp(at: 0.1) == .scheduleEndHold)
        #expect(interpreter.tapWindowExpired() == .endHoldNow)
    }

    @Test("Cancel clears hold and hands-free states")
    func cancelClearsState() {
        var interpreter = HotkeyGestureInterpreter()

        #expect(interpreter.keyDown(at: 0, isRepeat: false) == .beginHold)
        #expect(interpreter.cancel() == .cancel)
        #expect(interpreter.keyUp(at: 1) == nil)

        #expect(interpreter.keyDown(at: 2, isRepeat: false) == .beginHold)
    }
}
