import AppKit
import SwiftUI

enum ConverterArtwork: String {
    case inputRoute = "input-route"
    case componentsWorkshop = "components-workshop"
    case dryRunFlow = "dry-run-flow"
    case rollbackHarbor = "rollback-harbor"
    case cecMailbox = "cec-mailbox"
}

struct SceneArtworkView: View {
    let artwork: ConverterArtwork

    var body: some View {
        ZStack {
            if let image = NSImage(contentsOf: artworkURL) {
                Image(nsImage: image)
                    .resizable()
                    .aspectRatio(contentMode: .fill)
                    .accessibilityHidden(true)
            } else {
                Color(nsColor: .windowBackgroundColor)
                Image(systemName: "arrow.left.arrow.right.square")
                    .font(.system(size: 38, weight: .medium))
                    .foregroundStyle(.secondary)
                    .accessibilityHidden(true)
            }
        }
        .clipped()
    }

    private var artworkURL: URL {
        let filename = artwork.rawValue
        if let bundled = Bundle.main.url(forResource: filename, withExtension: "png", subdirectory: "Artwork") {
            return bundled
        }
        if let configuredRoot = ProcessInfo.processInfo.environment["MH3G_CONVERTER_ARTWORK_ROOT"] {
            return URL(fileURLWithPath: configuredRoot).appendingPathComponent("\(filename).png")
        }
        return URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .appendingPathComponent("Resources/Artwork/\(filename).png")
    }
}
