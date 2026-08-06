import SwiftUI
import ConverterPresentation

struct ConversionWorkbenchView: View {
    @Binding var localeOverride: String
    @State private var selectedNavigation: ConverterNavigation? = .input
    @State private var workflow = ConversionWorkflow(executable: ConverterExecutableLocator.locate())
    @Environment(\.accessibilityReduceMotion) private var reduceMotion

    private var language: ConverterLanguage {
        let override = ConverterLanguage(rawValue: localeOverride) ?? .system
        return override == .system
            ? (Locale.current.identifier.lowercased().hasPrefix("zh") ? .zhHans : .english)
            : override
    }

    var body: some View {
        NavigationSplitView {
            List(ConverterNavigation.allCases, selection: $selectedNavigation) { item in
                Label(ConverterCopy.text(item.titleKey, language: language), systemImage: item.systemImage)
                    .tag(item)
                    .accessibilityIdentifier(item.accessibilityIdentifier)
            }
            .navigationTitle(ConverterCopy.text("App.Title", language: language))
            .listStyle(.sidebar)
        } detail: {
            VStack(spacing: 0) {
                WorkflowStatusBanner(workflow: workflow, language: language)
                Divider()
                detailView
            }
            .frame(minWidth: 680, minHeight: 560)
        }
        .toolbar {
            ToolbarItem(placement: .principal) {
                HStack(spacing: 8) {
                    Image(systemName: "arrow.left.arrow.right.circle.fill")
                        .foregroundStyle(.tint)
                    Text(ConverterCopy.text("App.Title", language: language))
                        .font(.headline)
                }
            }
            ToolbarItem(placement: .automatic) {
                StatusPill(workflow: workflow, language: language)
            }
        }
        .onChange(of: selectedNavigation) { _, _ in
            guard !reduceMotion else { return }
            // Navigation is controlled by native split-view selection.  The
            // content changes are brief and do not disable any other control.
        }
    }

    @ViewBuilder
    private var detailView: some View {
        switch selectedNavigation ?? .input {
        case .input:
            InputInspectionView(workflow: workflow, language: language, navigation: $selectedNavigation)
        case .components:
            ComponentSelectionView(workflow: workflow, language: language, navigation: $selectedNavigation)
        case .dryRun:
            DryRunView(workflow: workflow, language: language, navigation: $selectedNavigation)
        case .writeRollback:
            WriteRollbackView(workflow: workflow, language: language, navigation: $selectedNavigation)
        case .history:
            ConversionHistoryView(workflow: workflow, language: language)
        case .experimentalCEC:
            ExperimentalCECView(workflow: workflow, language: language)
        case .settings:
            SettingsView(localeOverride: $localeOverride, workflow: workflow, language: language)
        }
    }
}

enum ConverterExecutableLocator {
    static func locate() -> URL {
        let bundled = Bundle.main.bundleURL
            .appendingPathComponent("Contents/MacOS/mh3g-save-convert")
        if FileManager.default.isExecutableFile(atPath: bundled.path) {
            return bundled
        }
        if let configured = ProcessInfo.processInfo.environment["MH3G_CONVERTER_CLI"], !configured.isEmpty {
            return URL(fileURLWithPath: configured)
        }
        return URL(fileURLWithPath: "/usr/local/bin/mh3g-save-convert")
    }
}

private struct StatusPill: View {
    let workflow: ConversionWorkflow
    let language: ConverterLanguage

    var body: some View {
        Label(title, systemImage: statusAppearance.image)
            .labelStyle(.titleAndIcon)
            .font(.caption)
            .foregroundStyle(statusAppearance.color)
            .padding(.horizontal, 9)
            .padding(.vertical, 5)
            .background(.quaternary, in: Capsule())
            .accessibilityLabel(title)
    }

    private var title: String {
        ConverterCopy.text(workflow.statusPresentation.titleKey, language: language)
    }

    private var statusAppearance: WorkflowStatusAppearance {
        WorkflowStatusAppearance(workflow.statusPresentation.kind)
    }
}

private struct WorkflowStatusBanner: View {
    let workflow: ConversionWorkflow
    let language: ConverterLanguage
    @Environment(\.accessibilityReduceMotion) private var reduceMotion
    @Environment(\.accessibilityReduceTransparency) private var reduceTransparency
    @Environment(\.accessibilityDifferentiateWithoutColor) private var differentiateWithoutColor
    @Environment(\.colorSchemeContrast) private var contrast

    private var presentation: WorkflowStatusPresentation { workflow.statusPresentation }
    private var appearance: WorkflowStatusAppearance { WorkflowStatusAppearance(presentation.kind) }

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: appearance.image)
                .font(.title3.weight(.semibold))
                .foregroundStyle(appearance.color)
                .symbolEffect(.pulse, options: .repeating, isActive: presentation.kind == .running && !reduceMotion)
                .frame(width: 28, height: 28)
            VStack(alignment: .leading, spacing: 3) {
                Text(ConverterCopy.text(presentation.titleKey, language: language))
                    .font(.headline)
                Text(ConverterCopy.text(presentation.detailKey, language: language))
                    .font(.subheadline)
                    .foregroundStyle(.secondary)
                    .fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 12)
            if presentation.isBlocking || differentiateWithoutColor {
                Text(presentation.isBlocking
                    ? ConverterCopy.text("Status.Blocked", language: language)
                    : ConverterCopy.text("Status.Ready", language: language))
                    .font(.caption.weight(.semibold))
                    .padding(.horizontal, 8)
                    .padding(.vertical, 4)
                    .background(appearance.color.opacity(0.12), in: Capsule())
                    .foregroundStyle(appearance.color)
            }
        }
        .padding(.horizontal, 18)
        .padding(.vertical, 12)
        .background {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .fill(reduceTransparency ? Color(nsColor: .controlBackgroundColor) : .clear)
                .background(
                    reduceTransparency ? AnyShapeStyle(.clear) : AnyShapeStyle(.thinMaterial),
                    in: RoundedRectangle(cornerRadius: 14, style: .continuous)
                )
        }
        .overlay {
            RoundedRectangle(cornerRadius: 14, style: .continuous)
                .stroke(contrast == .increased ? Color.primary : appearance.color.opacity(0.24), lineWidth: contrast == .increased ? 2 : 1)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 10)
        .accessibilityElement(children: .combine)
        .accessibilityIdentifier("mh3g.converter.status.\(presentation.kind.rawValue)")
        .animation(reduceMotion ? nil : .spring(response: 0.34, dampingFraction: 1), value: presentation.kind)
    }
}

private struct WorkflowStatusAppearance {
    let image: String
    let color: Color

    init(_ kind: WorkflowStatusKind) {
        switch kind {
        case .needsInput:
            image = "doc.badge.plus"
            color = .secondary
        case .needsInspection:
            image = "doc.text.magnifyingglass"
            color = .blue
        case .readyForDryRun:
            image = "checkmark.shield"
            color = .blue
        case .blocked:
            image = "lock.trianglebadge.exclamationmark"
            color = .orange
        case .authorized:
            image = "checkmark.shield.fill"
            color = .blue
        case .running:
            image = "arrow.triangle.2.circlepath"
            color = .orange
        case .succeeded:
            image = "checkmark.circle.fill"
            color = .green
        case .failed:
            image = "exclamationmark.triangle.fill"
            color = .red
        }
    }
}
