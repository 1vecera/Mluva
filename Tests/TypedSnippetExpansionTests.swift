import CoreGraphics
import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Typed snippet expansion")
struct TypedSnippetExpansionTests {
    @Test("Delimiter expands an exact typed trigger")
    func expandsExactTrigger() {
        let store = makeStore()
        store.saveSnippet(
            trigger: "email signoff",
            typedTrigger: ";sig",
            expansion: "{{date}} costs $5"
        )
        let replacer = RecordingTypedSnippetReplacer()
        let manager = GlobalHotkeyManager(
            personalizationStore: store,
            typedSnippetReplacer: replacer,
            snippetContext: {
                TypedSnippetApplicationContext(
                    bundleIdentifier: "com.apple.Notes",
                    isSecureTextField: false
                )
            },
            snippetVariables: { ["date": "July 31, 2026"] }
        )
        manager.shouldExpandTypedSnippets = { true }

        for character in ";sig" {
            #expect(!manager.handleTypedSnippetInput(
                String(character),
                keyCode: 0,
                flags: []
            ))
        }
        let consumed = manager.handleTypedSnippetInput(" ", keyCode: 49, flags: [])

        #expect(consumed)
        #expect(replacer.replacements == [TypedSnippetReplacement(
            characterCount: 4,
            text: "July 31, 2026 costs $5",
            trailingText: " "
        )])
    }

    @Test("Secure fields never buffer or expand typed snippets")
    func secureFieldsDoNotExpand() {
        let store = makeStore()
        store.saveSnippet(
            trigger: "password helper",
            typedTrigger: ";secret",
            expansion: "must not appear"
        )
        let replacer = RecordingTypedSnippetReplacer()
        let manager = GlobalHotkeyManager(
            personalizationStore: store,
            typedSnippetReplacer: replacer,
            snippetContext: {
                TypedSnippetApplicationContext(
                    bundleIdentifier: "com.apple.Safari",
                    isSecureTextField: true
                )
            }
        )
        manager.shouldExpandTypedSnippets = { true }

        for character in ";secret " {
            _ = manager.handleTypedSnippetInput(
                String(character),
                keyCode: character == " " ? 49 : 0,
                flags: []
            )
        }

        #expect(replacer.replacements.isEmpty)
    }

    @Test("Disabled expansion never carries typed content into a later opt-in")
    func disabledExpansionDoesNotBuffer() {
        let store = makeStore()
        store.saveSnippet(
            trigger: "signature",
            typedTrigger: ";sig",
            expansion: "Signature"
        )
        let replacer = RecordingTypedSnippetReplacer()
        var enabled = false
        let manager = GlobalHotkeyManager(
            personalizationStore: store,
            typedSnippetReplacer: replacer,
            snippetContext: {
                TypedSnippetApplicationContext(
                    bundleIdentifier: "com.apple.Notes",
                    isSecureTextField: false
                )
            }
        )
        manager.shouldExpandTypedSnippets = { enabled }

        for character in ";sig" {
            _ = manager.handleTypedSnippetInput(String(character), keyCode: 0, flags: [])
        }
        enabled = true
        _ = manager.handleTypedSnippetInput(" ", keyCode: 49, flags: [])

        #expect(replacer.replacements.isEmpty)
    }

    @Test("Starting dictation clears an unfinished typed trigger")
    func dictationGestureClearsBuffer() throws {
        let store = makeStore()
        store.saveSnippet(
            trigger: "signature",
            typedTrigger: ";sig",
            expansion: "Signature"
        )
        let replacer = RecordingTypedSnippetReplacer()
        let manager = GlobalHotkeyManager(
            personalizationStore: store,
            typedSnippetReplacer: replacer,
            snippetContext: {
                TypedSnippetApplicationContext(
                    bundleIdentifier: "com.apple.Notes",
                    isSecureTextField: false
                )
            }
        )
        manager.shouldExpandTypedSnippets = { true }

        for character in ";sig" {
            _ = manager.handleTypedSnippetInput(String(character), keyCode: 0, flags: [])
        }
        let hotkey = try #require(CGEvent(
            keyboardEventSource: nil,
            virtualKey: UInt16(AppSettings.defaultHotkeyKeyCode),
            keyDown: true
        ))
        hotkey.flags = CGEventFlags(rawValue: AppSettings.defaultHotkeyModifiers)
        #expect(!manager.handleGestureEvent(type: .flagsChanged, event: hotkey))
        _ = manager.handleTypedSnippetInput(" ", keyCode: 49, flags: [])

        #expect(replacer.replacements.isEmpty)
    }

    @Test("Application scope and word boundaries prevent accidental expansion")
    func respectsScopeAndBoundaries() {
        let store = makeStore()
        store.saveSnippet(
            trigger: "signature",
            typedTrigger: "sig",
            expansion: "Signature",
            bundleIdentifier: "com.apple.mail"
        )
        let replacer = RecordingTypedSnippetReplacer()
        var bundleIdentifier = "com.apple.Notes"
        let manager = GlobalHotkeyManager(
            personalizationStore: store,
            typedSnippetReplacer: replacer,
            snippetContext: {
                TypedSnippetApplicationContext(
                    bundleIdentifier: bundleIdentifier,
                    isSecureTextField: false
                )
            }
        )
        manager.shouldExpandTypedSnippets = { true }

        for character in "assign " {
            _ = manager.handleTypedSnippetInput(String(character), keyCode: 0, flags: [])
        }
        bundleIdentifier = "com.apple.mail"
        for character in "sig " {
            _ = manager.handleTypedSnippetInput(String(character), keyCode: 0, flags: [])
        }

        #expect(replacer.replacements.map(\.text) == ["Signature"])
    }

    @Test("Application typed triggers override global triggers")
    func applicationTriggerOverridesGlobal() {
        let store = makeStore()
        store.saveSnippet(
            trigger: "personal signature",
            typedTrigger: ";sig",
            expansion: "Global"
        )
        store.saveSnippet(
            trigger: "work signature",
            typedTrigger: ";sig",
            expansion: "Mail",
            bundleIdentifier: "com.apple.mail"
        )
        let replacer = RecordingTypedSnippetReplacer()
        let manager = GlobalHotkeyManager(
            personalizationStore: store,
            typedSnippetReplacer: replacer,
            snippetContext: {
                TypedSnippetApplicationContext(
                    bundleIdentifier: "com.apple.mail",
                    isSecureTextField: false
                )
            }
        )
        manager.shouldExpandTypedSnippets = { true }

        for character in ";sig " {
            _ = manager.handleTypedSnippetInput(
                String(character),
                keyCode: character == " " ? 49 : 0,
                flags: []
            )
        }

        #expect(replacer.replacements.map(\.text) == ["Mail"])
    }

    private func makeStore() -> PersonalizationStore {
        PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("typed-snippets-\(UUID().uuidString).json")
        )
    }
}

private final class RecordingTypedSnippetReplacer: TypedSnippetReplacing {
    private(set) var replacements: [TypedSnippetReplacement] = []

    func replaceTypedSnippet(_ replacement: TypedSnippetReplacement) {
        replacements.append(replacement)
    }
}
