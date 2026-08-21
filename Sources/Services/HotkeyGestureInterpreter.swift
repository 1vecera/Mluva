import Foundation

enum HotkeyGestureAction: Equatable, Sendable {
    case beginHold
    case scheduleEndHold
    case endHoldNow
    case continueHandsFree
    case endHandsFree
    case cancel
}

struct HotkeyGestureInterpreter {
    private enum State: Equatable {
        case idle
        case holding(startedAt: TimeInterval)
        case awaitingSecondTap(releasedAt: TimeInterval)
        case handsFree
    }

    let shortTapMaximumDuration: TimeInterval
    let doubleTapWindow: TimeInterval
    private var state: State = .idle

    init(
        shortTapMaximumDuration: TimeInterval = 0.2,
        doubleTapWindow: TimeInterval = 0.3
    ) {
        self.shortTapMaximumDuration = shortTapMaximumDuration
        self.doubleTapWindow = doubleTapWindow
    }

    mutating func keyDown(
        at timestamp: TimeInterval,
        isRepeat: Bool
    ) -> HotkeyGestureAction? {
        guard !isRepeat else { return nil }

        switch state {
        case .idle:
            state = .holding(startedAt: timestamp)
            return .beginHold

        case .holding:
            return nil

        case .awaitingSecondTap(let releasedAt):
            if timestamp - releasedAt <= doubleTapWindow {
                state = .handsFree
                return .continueHandsFree
            }
            state = .holding(startedAt: timestamp)
            return .beginHold

        case .handsFree:
            state = .idle
            return .endHandsFree
        }
    }

    mutating func keyUp(at timestamp: TimeInterval) -> HotkeyGestureAction? {
        guard case .holding(let startedAt) = state else { return nil }

        if timestamp - startedAt <= shortTapMaximumDuration {
            state = .awaitingSecondTap(releasedAt: timestamp)
            return .scheduleEndHold
        }
        state = .idle
        return .endHoldNow
    }

    mutating func tapWindowExpired() -> HotkeyGestureAction? {
        guard case .awaitingSecondTap = state else { return nil }
        state = .idle
        return .endHoldNow
    }

    mutating func cancel() -> HotkeyGestureAction? {
        guard state != .idle else { return nil }
        state = .idle
        return .cancel
    }
}
