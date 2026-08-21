import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Personalization store")
struct PersonalizationStoreTests {
    @Test("Dictionary and snippets persist together")
    func persistsPersonalization() throws {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)

        store.saveDictionaryReplacement(spoken: "post grass", written: "Postgres")
        store.saveSnippet(trigger: "email signoff", expansion: "Best,\nDaniel")

        let reloaded = PersonalizationStore(fileURL: fileURL)
        #expect(reloaded.dictionary.map(\.spoken) == ["post grass"])
        #expect(reloaded.dictionary.map(\.written) == ["Postgres"])
        #expect(reloaded.snippets.map(\.trigger) == ["email signoff"])
        #expect(reloaded.snippets.map(\.expansion) == ["Best,\nDaniel"])
        let fileAttributes = try FileManager.default.attributesOfItem(atPath: fileURL.path)
        #expect((fileAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
        let directoryAttributes = try FileManager.default.attributesOfItem(
            atPath: fileURL.deletingLastPathComponent().path
        )
        #expect((directoryAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o700)
    }

    @Test("Dictionary case behavior survives restart")
    func persistsDictionaryCaseBehavior() throws {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)

        store.saveDictionaryReplacement(
            spoken: "example value",
            written: "replacement text",
            caseBehavior: .matchSpoken
        )

        let replacement = try #require(
            PersonalizationStore(fileURL: fileURL).dictionary.first
        )
        #expect(replacement.caseBehavior == .matchSpoken)
    }

    @Test("Spoken and typed snippet triggers survive restart")
    func persistsTypedSnippetTrigger() throws {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)

        store.saveSnippet(
            trigger: "email signoff",
            typedTrigger: ";signoff",
            expansion: "Best,\nDaniel"
        )

        let snippet = try #require(
            PersonalizationStore(fileURL: fileURL).snippets.first
        )
        #expect(snippet.trigger == "email signoff")
        #expect(snippet.typedTrigger == ";signoff")
    }

    @Test("Saving the same typed trigger updates its scoped snippet")
    func upsertsTypedSnippetTrigger() {
        let store = PersonalizationStore(fileURL: temporaryFileURL())

        store.saveSnippet(
            trigger: "short signature",
            typedTrigger: ";sig",
            expansion: "First"
        )
        store.saveSnippet(
            trigger: "full signature",
            typedTrigger: ";sig",
            expansion: "Second"
        )

        #expect(store.snippets.count == 1)
        #expect(store.snippets.first?.trigger == "full signature")
        #expect(store.snippets.first?.expansion == "Second")
    }

    @Test("Typed triggers are single tokens")
    func rejectsTypedTriggerWhitespace() {
        let store = PersonalizationStore(fileURL: temporaryFileURL())

        store.saveSnippet(
            trigger: "signature",
            typedTrigger: "two words",
            expansion: "Signature"
        )

        #expect(store.snippets.isEmpty)
        #expect(store.persistenceError == "Typed triggers cannot contain whitespace.")
    }

    @Test("Dismissed vocabulary suggestions survive restart")
    func persistsDismissedVocabularySuggestions() {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)

        store.dismissVocabularySuggestion(id: "com.apple.Notes\u{1F}post grass\u{1F}Postgres")

        let reloaded = PersonalizationStore(fileURL: fileURL)
        #expect(reloaded.dismissedVocabularySuggestionIDs == [
            "com.apple.Notes\u{1F}post grass\u{1F}Postgres",
        ])
    }

    @Test("Saving the same spoken phrase updates instead of duplicating")
    func upsertsDictionaryPhrase() {
        let store = PersonalizationStore(fileURL: temporaryFileURL())

        store.saveDictionaryReplacement(spoken: "g c p", written: "GCP")
        store.saveDictionaryReplacement(spoken: "G C P", written: "Google Cloud")

        #expect(store.dictionary.count == 1)
        #expect(store.dictionary[0].written == "Google Cloud")
    }

    @Test("Blank personalization entries are rejected")
    func rejectsBlankEntries() {
        let store = PersonalizationStore(fileURL: temporaryFileURL())

        store.saveDictionaryReplacement(spoken: " ", written: "value")
        store.saveSnippet(trigger: "trigger", expansion: " ")

        #expect(store.dictionary.isEmpty)
        #expect(store.snippets.isEmpty)
    }

    @Test("Application-specific personalization overrides the global phrase")
    func scopesPersonalizationToApplication() {
        let store = PersonalizationStore(fileURL: temporaryFileURL())
        store.saveDictionaryReplacement(spoken: "ship it", written: "Ship it")
        store.saveDictionaryReplacement(
            spoken: "ship it",
            written: "git push",
            bundleIdentifier: "com.microsoft.VSCode"
        )
        store.saveSnippet(trigger: "review", expansion: "Please review")
        store.saveSnippet(
            trigger: "review",
            expansion: "gh pr view --web",
            bundleIdentifier: "com.microsoft.VSCode"
        )

        let editorConfiguration = store.processingConfiguration(
            removeFillers: false,
            targetBundleIdentifier: "com.microsoft.VSCode"
        )
        let mailConfiguration = store.processingConfiguration(
            removeFillers: false,
            targetBundleIdentifier: "com.apple.mail"
        )

        #expect(editorConfiguration.dictionary.map(\.written) == ["git push"])
        #expect(editorConfiguration.snippets.map(\.expansion) == ["gh pr view --web"])
        #expect(mailConfiguration.dictionary.map(\.written) == ["Ship it"])
        #expect(mailConfiguration.snippets.map(\.expansion) == ["Please review"])
    }

    @Test("Built-in and custom writing styles survive restart")
    func persistsSavedStyles() throws {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)

        #expect(Set(store.styles.map(\.name)) == [
            "Email",
            "Google Chat",
            "Message",
            "Prose",
            "Prompt",
            "Tasks",
            "Technical notes",
        ])

        store.saveStyle(
            name: "Release note",
            instructions: "Turn the draft into a concise customer-facing release note."
        )

        let reloaded = PersonalizationStore(fileURL: fileURL)
        let customStyle = try #require(
            reloaded.styles.first { $0.name == "Release note" }
        )
        #expect(!customStyle.isBuiltIn)
        #expect(customStyle.instructions.contains("customer-facing"))
    }

    @Test("Custom output modes can be edited in place")
    func editsCustomOutputMode() throws {
        let store = PersonalizationStore(fileURL: temporaryFileURL())
        let id = try #require(store.saveStyle(
            name: "Standup",
            instructions: "Make a standup update."
        ))

        #expect(store.updateStyle(
            id: id,
            name: "Daily standup",
            instructions: "Use compact bullets for done, next, and blockers."
        ))
        #expect(store.style(id: id)?.name == "Daily standup")
        #expect(store.style(id: id)?.instructions.contains("blockers") == true)
    }

    @Test("Style selection can follow the target application")
    func remembersStylePerApplication() throws {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)
        let email = try #require(store.styles.first { $0.name == "Email" })
        let technical = try #require(store.styles.first { $0.name == "Technical notes" })

        store.selectStyle(
            email.id,
            for: "com.apple.mail",
            rememberPerApplication: true
        )
        store.selectStyle(
            technical.id,
            for: "com.microsoft.VSCode",
            rememberPerApplication: true
        )

        let reloaded = PersonalizationStore(fileURL: fileURL)
        #expect(reloaded.selectedStyle(
            for: "com.apple.mail",
            rememberPerApplication: true
        )?.name == "Email")
        #expect(reloaded.selectedStyle(
            for: "com.microsoft.VSCode",
            rememberPerApplication: true
        )?.name == "Technical notes")
        #expect(reloaded.selectedStyle(
            for: "com.apple.Notes",
            rememberPerApplication: true
        ) == nil)
    }

    @Test("Mode and provider selections follow the target application")
    func remembersApplicationProfiles() {
        let fileURL = temporaryFileURL()
        let store = PersonalizationStore(fileURL: fileURL)

        store.selectMode(.command, for: "com.apple.mail", rememberPerApplication: true)
        store.selectProvider(.googleCloud, for: "com.apple.mail", rememberPerApplication: true)
        store.selectMode(.scratchpad, for: "com.microsoft.VSCode", rememberPerApplication: true)
        store.selectProvider(.apple, for: "com.microsoft.VSCode", rememberPerApplication: true)

        let reloaded = PersonalizationStore(fileURL: fileURL)
        #expect(reloaded.selectedMode(
            for: "com.apple.mail",
            rememberPerApplication: true,
            fallback: .dictation
        ) == .command)
        #expect(reloaded.selectedProvider(
            for: "com.apple.mail",
            rememberPerApplication: true,
            fallback: .automatic
        ) == .googleCloud)
        #expect(reloaded.selectedMode(
            for: "com.microsoft.VSCode",
            rememberPerApplication: true,
            fallback: .dictation
        ) == .scratchpad)
        #expect(reloaded.selectedProvider(
            for: "com.apple.Notes",
            rememberPerApplication: true,
            fallback: .automatic
        ) == .automatic)
    }

    private func temporaryFileURL() -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent("personalization-\(UUID().uuidString)", isDirectory: true)
            .appendingPathComponent("personalization.json")
    }
}
