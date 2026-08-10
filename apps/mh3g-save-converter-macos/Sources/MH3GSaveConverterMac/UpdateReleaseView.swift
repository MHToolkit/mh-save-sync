import AppKit
import ConverterPresentation
import SwiftUI

struct UpdateReleaseView: View {
    let release: GitHubConverterRelease
    let currentVersion: String
    let language: ConverterLanguage

    @Environment(\.dismiss) private var dismiss

    var body: some View {
        VStack(alignment: .leading, spacing: 18) {
            HStack(alignment: .top, spacing: 14) {
                Image(systemName: "arrow.down.circle.fill")
                    .font(.system(size: 34))
                    .foregroundStyle(.tint)
                    .accessibilityHidden(true)
                VStack(alignment: .leading, spacing: 4) {
                    Text(ConverterCopy.text("Update.AvailableTitle", language: language))
                        .font(.title2.weight(.semibold))
                    Text(releaseTitle)
                        .font(.headline)
                    Text(
                        String(
                            format: ConverterCopy.text("Update.VersionSummary", language: language),
                            currentVersion,
                            release.tagName
                        )
                    )
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    if let published = formattedPublishedDate {
                        Text(published)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                    }
                }
            }

            Divider()

            Text(ConverterCopy.text("Update.ReleaseNotes", language: language))
                .font(.headline)
            ScrollView {
                Text(releaseNotes)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .textSelection(.enabled)
            }
            .frame(minHeight: 150, maxHeight: 320)

            HStack {
                Spacer()
                Button(ConverterCopy.text("Update.Later", language: language)) {
                    dismiss()
                }
                Button(ConverterCopy.text("Update.OpenRelease", language: language)) {
                    NSWorkspace.shared.open(release.htmlURL)
                    dismiss()
                }
                .keyboardShortcut(.defaultAction)
            }
        }
        .padding(24)
        .frame(minWidth: 540, idealWidth: 600, minHeight: 390)
    }

    private var formattedPublishedDate: String? {
        guard let value = release.publishedAt,
              let date = ISO8601DateFormatter().date(from: value)
        else {
            return nil
        }
        return date.formatted(date: .long, time: .omitted)
    }

    private var releaseTitle: String {
        guard let name = release.name?.trimmingCharacters(in: .whitespacesAndNewlines),
              !name.isEmpty
        else {
            return release.tagName
        }
        return name
    }

    private var releaseNotes: String {
        guard let body = release.body?.trimmingCharacters(in: .whitespacesAndNewlines),
              !body.isEmpty
        else {
            return ConverterCopy.text("Update.NoReleaseNotes", language: language)
        }
        return body
    }
}
