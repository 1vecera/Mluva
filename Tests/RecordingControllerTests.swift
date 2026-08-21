import Testing
import Foundation
@testable import VoiceScribeMac

@Suite("Recording Controller")
@MainActor
struct RecordingControllerTests {
    // Isolated settings per test — no global state pollution
    let settings: AppSettings
    let controller: RecordingController

    init() {
        let defaults = UserDefaults(suiteName: "test-rc-\(UUID().uuidString)")!
        settings = AppSettings(defaults: defaults)
        controller = RecordingController(settings: settings)
    }

    // MARK: - Initial State

    @Test("Initial state is idle with empty fields")
    func initialState() {
        #expect(controller.state == .idle)
        #expect(controller.partialText.isEmpty)
        #expect(controller.lastCommittedText.isEmpty)
        #expect(controller.error == nil)
    }

    // MARK: - Filler Removal (enabled by default)

    @Test("Removes 'um' filler word")
    func removesUm() {
        #expect(controller.removeFiller("I um think so") == "I think so")
    }

    @Test("Removes 'uh' filler word")
    func removesUh() {
        #expect(controller.removeFiller("uh hello uh world") == "hello world")
    }

    @Test("Removes 'hmm' filler word")
    func removesHmm() {
        #expect(controller.removeFiller("hmm let me think") == "let me think")
    }

    @Test("Removes 'mhm' filler word")
    func removesMhm() {
        #expect(controller.removeFiller("mhm that's right") == "that's right")
    }

    @Test("Removes 'uh huh' filler phrase")
    func removesUhHuh() {
        #expect(controller.removeFiller("uh huh yes") == "yes")
    }

    @Test("Removes 'mm' filler word")
    func removesMm() {
        #expect(controller.removeFiller("mm I see") == "I see")
    }

    @Test("Filler removal is case insensitive")
    func caseInsensitive() {
        #expect(controller.removeFiller("UM okay UH sure") == "okay sure")
    }

    @Test("Non-filler text is preserved intact")
    func preservesCleanText() {
        #expect(controller.removeFiller("this is a clean sentence") == "this is a clean sentence")
    }

    @Test("Multiple spaces are collapsed after removal")
    func collapsesSpaces() {
        #expect(controller.removeFiller("well um uh okay") == "well okay")
    }

    @Test("All-filler text returns empty string")
    func allFillerReturnsEmpty() {
        #expect(controller.removeFiller("um uh hmm") == "")
    }

    // MARK: - Filler Removal (disabled)

    @Test("When removeFiller=false, text passes through unchanged")
    func fillerRemovalDisabled() {
        settings.removeFiller = false
        #expect(controller.removeFiller("I um think so") == "I um think so")
    }

    @Test("When removeFiller is re-enabled, fillers are removed again")
    func fillerRemovalReEnabled() {
        settings.removeFiller = false
        #expect(controller.removeFiller("um test") == "um test")

        settings.removeFiller = true
        #expect(controller.removeFiller("um test") == "test")
    }
}
