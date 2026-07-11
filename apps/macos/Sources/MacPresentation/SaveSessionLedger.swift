import Foundation

public struct SaveSessionLedger: Codable, Equatable, Sendable {
    public struct Entry: Codable, Equatable, Sendable {
        public var establishedHead: String?
        public var sessionBaseHead: String?
        public var sessionObservationKnown: Bool

        public init(
            establishedHead: String? = nil,
            sessionBaseHead: String? = nil,
            sessionObservationKnown: Bool = false
        ) {
            self.establishedHead = establishedHead
            self.sessionBaseHead = sessionBaseHead
            self.sessionObservationKnown = sessionObservationKnown
        }
    }

    public private(set) var saves: [String: Entry]

    public init(saves: [String: Entry] = [:]) {
        self.saves = saves
    }

    /// Freezes the cloud HEAD seen before gameplay. A failed observation is
    /// represented explicitly and must never be replaced by a later status.
    public mutating func beginSession(logicalSaveID: String, observedCloudHead: String?) {
        var entry = saves[logicalSaveID] ?? Entry()
        entry.sessionBaseHead = observedCloudHead
        entry.sessionObservationKnown = observedCloudHead != nil
        saves[logicalSaveID] = entry
    }

    /// Status refreshes are informational only; they cannot rewrite a session base.
    public mutating func observeStatus(logicalSaveID: String, cloudHead: String?) {
        guard var entry = saves[logicalSaveID] else { return }
        if entry.establishedHead == nil, !entry.sessionObservationKnown {
            entry.establishedHead = cloudHead
        }
        saves[logicalSaveID] = entry
    }

    /// Restore/up-to-date/fast-forward/explicit replace prove local and cloud HEAD.
    public mutating func recordEstablishedHead(logicalSaveID: String, head: String) {
        saves[logicalSaveID] = Entry(
            establishedHead: head,
            sessionBaseHead: head,
            sessionObservationKnown: true
        )
    }

    public func baseHeadForUpload(logicalSaveID: String) -> String? {
        let entry = saves[logicalSaveID]
        return entry?.sessionObservationKnown == true ? entry?.sessionBaseHead : nil
    }
}
