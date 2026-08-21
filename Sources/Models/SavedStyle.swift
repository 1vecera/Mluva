import Foundation

struct SavedStyle: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let name: String
    let instructions: String
    let isBuiltIn: Bool

    init(
        id: UUID = UUID(),
        name: String,
        instructions: String,
        isBuiltIn: Bool = false
    ) {
        self.id = id
        self.name = name
        self.instructions = instructions
        self.isBuiltIn = isBuiltIn
    }

    static let builtIns: [SavedStyle] = [
        SavedStyle(
            id: UUID(uuidString: "FDD26FEF-80E7-4AD3-9D08-7874266A12E8")!,
            name: "Message",
            instructions: "Rewrite as a concise, natural chat message. Preserve every fact, request, technical term, and level of certainty.",
            isBuiltIn: true
        ),
        SavedStyle(
            id: UUID(uuidString: "5DD73A99-80C2-4FA9-99C2-DC7528300248")!,
            name: "Google Chat",
            instructions: "Rewrite as a concise, natural Google Chat message in the speaker's language. Lead with the point or request, use short paragraphs, and use bullets only when they improve scanability. Preserve every fact, constraint, name, and level of certainty. Do not invent a greeting, recipient, emoji, or sign-off.",
            isBuiltIn: true
        ),
        SavedStyle(
            id: UUID(uuidString: "7AD20551-3DC5-4774-9D34-DB4704F3A05F")!,
            name: "Tasks",
            instructions: "Rewrite as task-ready text with a short action-oriented title followed by compact bullets for the outcome, context, and constraints that were actually dictated. Preserve explicit owners and dates. Never invent an assignee, deadline, priority, or acceptance criterion.",
            isBuiltIn: true
        ),
        SavedStyle(
            id: UUID(uuidString: "0E5FB9A9-56CF-4C6F-A825-26D4057AB5AB")!,
            name: "Email",
            instructions: "Rewrite as a clear professional email body. Preserve every fact and request. Do not invent a greeting, recipient, or sign-off.",
            isBuiltIn: true
        ),
        SavedStyle(
            id: UUID(uuidString: "58D7C7D2-DDBA-41C8-B6B8-497220E9D5A9")!,
            name: "Prose",
            instructions: "Rewrite as polished connected prose with natural paragraphs. Preserve meaning, facts, and the speaker's level of certainty.",
            isBuiltIn: true
        ),
        SavedStyle(
            id: UUID(uuidString: "84D3302E-8886-4079-88C1-A86F1B4BE7D6")!,
            name: "Technical notes",
            instructions: "Rewrite as compact structured technical notes. Preserve commands, identifiers, paths, URLs, versions, numbers, and negation exactly.",
            isBuiltIn: true
        ),
        SavedStyle(
            id: UUID(uuidString: "54680987-230B-4E27-851C-AAD358AA54B3")!,
            name: "Prompt",
            instructions: "Rewrite as a clear reusable instruction for an AI system. Preserve constraints and source facts. Do not answer the instruction.",
            isBuiltIn: true
        ),
    ]
}

enum TranscriptStyleOutcome: String, Codable, Equatable, Sendable {
    case notRequested
    case applied
    case rejectedUnsafe
    case unavailable
}

struct TranscriptStyleRequest: Equatable, Sendable {
    let text: String
    let style: SavedStyle
    let context: TranscriptContext
    let protectedVocabulary: [String]

    var targetApplicationName: String? { context.applicationName }

    init(
        text: String,
        style: SavedStyle,
        targetApplicationName: String? = nil,
        context: TranscriptContext? = nil,
        protectedVocabulary: [String] = []
    ) {
        self.text = text
        self.style = style
        self.context = context ?? TranscriptContext(applicationName: targetApplicationName)
        self.protectedVocabulary = protectedVocabulary
    }
}

struct TranscriptStyleResult: Equatable, Sendable {
    let text: String
    let outcome: TranscriptStyleOutcome
    let violations: [TranscriptIntegrityViolation]
}

protocol TranscriptStyleBackend: Sendable {
    func apply(_ request: TranscriptStyleRequest) async throws -> String
}

protocol TranscriptStyling: Sendable {
    func apply(_ request: TranscriptStyleRequest) async -> TranscriptStyleResult
}
