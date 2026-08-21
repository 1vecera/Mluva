import AppKit
import Foundation

struct PasteboardItemData: Equatable, Sendable {
    let values: [String: Data]
}

protocol PasteboardAccess: AnyObject {
    var items: [PasteboardItemData] { get }
    var changeCount: Int { get }

    @discardableResult
    func replaceItems(_ items: [PasteboardItemData]) -> Bool
}

struct ClipboardTransaction: Equatable, Sendable {
    let originalItems: [PasteboardItemData]
    let installedChangeCount: Int
}

final class ClipboardTransactionController {
    private static let plainTextType = NSPasteboard.PasteboardType.string.rawValue
    private let pasteboard: any PasteboardAccess

    init(pasteboard: any PasteboardAccess = SystemPasteboardAccess()) {
        self.pasteboard = pasteboard
    }

    func install(text: String) -> ClipboardTransaction? {
        let originalItems = pasteboard.items
        let item = PasteboardItemData(values: [
            Self.plainTextType: Data(text.utf8)
        ])
        guard pasteboard.replaceItems([item]) else {
            pasteboard.replaceItems(originalItems)
            return nil
        }
        return ClipboardTransaction(
            originalItems: originalItems,
            installedChangeCount: pasteboard.changeCount
        )
    }

    @discardableResult
    func restore(_ transaction: ClipboardTransaction) -> Bool {
        guard ownsInstalledText(transaction) else {
            return false
        }
        return pasteboard.replaceItems(transaction.originalItems)
    }

    func ownsInstalledText(_ transaction: ClipboardTransaction) -> Bool {
        pasteboard.changeCount == transaction.installedChangeCount
    }

    @discardableResult
    func copyOnly(text: String) -> Bool {
        let item = PasteboardItemData(values: [
            Self.plainTextType: Data(text.utf8)
        ])
        return pasteboard.replaceItems([item])
    }
}

final class SystemPasteboardAccess: PasteboardAccess {
    private let pasteboard: NSPasteboard

    init(pasteboard: NSPasteboard = .general) {
        self.pasteboard = pasteboard
    }

    var changeCount: Int { pasteboard.changeCount }

    var items: [PasteboardItemData] {
        (pasteboard.pasteboardItems ?? []).map { item in
            let values = item.types.reduce(into: [String: Data]()) { result, type in
                if let data = item.data(forType: type) {
                    result[type.rawValue] = data
                }
            }
            return PasteboardItemData(values: values)
        }
    }

    @discardableResult
    func replaceItems(_ items: [PasteboardItemData]) -> Bool {
        pasteboard.clearContents()
        guard !items.isEmpty else { return true }

        let pasteboardItems = items.map { itemData in
            let item = NSPasteboardItem()
            for (rawType, data) in itemData.values {
                item.setData(data, forType: NSPasteboard.PasteboardType(rawType))
            }
            return item
        }
        return pasteboard.writeObjects(pasteboardItems)
    }
}
