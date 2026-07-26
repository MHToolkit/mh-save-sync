import SwiftUI
import ConverterPresentation

struct ComponentSelectionView: View {
    @Bindable var workflow: ConversionWorkflow
    let language: ConverterLanguage

    var body: some View {
        WorkbenchPage(
            artwork: .componentsWorkshop,
            title: ConverterCopy.text("Navigation.Components", language: language),
            subtitle: ConverterCopy.text("Components.Subtitle", language: language)
        ) {
            Form {
                Section {
                    Toggle(ConverterCopy.text("Components.System", language: language), isOn: binding(\.includeSystem))
                    if workflow.components.includeSystem {
                        SelectedPathRow(
                            title: ConverterCopy.text("Components.SystemSource", language: language),
                            value: workflow.components.systemSource,
                            chooseTitle: ConverterCopy.text("Input.Select", language: language)
                        ) {
                            chooseSystemSource()
                        }
                        SelectedPathRow(
                            title: ConverterCopy.text("Components.SystemTarget", language: language),
                            value: workflow.components.systemTarget,
                            chooseTitle: ConverterCopy.text("Input.Select", language: language)
                        ) {
                            chooseSystemTarget()
                        }
                    }
                } footer: {
                    Text(ConverterCopy.text("Components.SystemFooter", language: language))
                }

                Section {
                    Toggle(ConverterCopy.text("Components.GuildCards", language: language), isOn: binding(\.includeGuildCards))
                    GroupScopeCaption(
                        image: "person.2.badge.gearshape",
                        names: ExtraGroup.guildCards.componentNames.joined(separator: " · "),
                        enabled: workflow.components.includeGuildCards,
                        language: language
                    )
                    Toggle(ConverterCopy.text("Components.Quests", language: language), isOn: binding(\.includeQuests))
                    GroupScopeCaption(
                        image: "scroll",
                        names: ExtraGroup.quests.componentNames.joined(separator: " · "),
                        enabled: workflow.components.includeQuests,
                        language: language
                    )
                    if !workflow.components.selectedGroups.isEmpty {
                        SelectedPathRow(
                            title: ConverterCopy.text("Components.ExtDataSource", language: language),
                            value: workflow.components.extraSourceDirectory,
                            chooseTitle: ConverterCopy.text("Input.Select", language: language)
                        ) {
                            chooseExtraSource()
                        }
                        SelectedPathRow(
                            title: ConverterCopy.text("Components.Staging", language: language),
                            value: workflow.components.extraStagingDirectory,
                            chooseTitle: ConverterCopy.text("Input.Select", language: language)
                        ) {
                            chooseExtraStaging()
                        }
                        SelectedPathRow(
                            title: ConverterCopy.text("Components.Target", language: language),
                            value: workflow.components.extraTargetDirectory,
                            chooseTitle: ConverterCopy.text("Input.Select", language: language)
                        ) {
                            chooseExtraTarget()
                        }
                    }
                } footer: {
                    Text(ConverterCopy.text("Components.ExtrasFooter", language: language))
                }
            }
            .formStyle(.grouped)
            .disabled(workflow.activeOperation != nil)
        }
    }

    private func binding(_ keyPath: WritableKeyPath<ComponentSelection, Bool>) -> Binding<Bool> {
        Binding(
            get: { workflow.components[keyPath: keyPath] },
            set: { value in
                var next = workflow.components
                next[keyPath: keyPath] = value
                workflow.setComponents(next)
            }
        )
    }

    private func chooseSystemSource() {
        guard let url = OpenPanel.selectFile(
            title: ConverterCopy.text("Components.SystemSource", language: language),
            message: ConverterCopy.text("Components.SystemSourceMessage", language: language)
        ) else { return }
        update { $0.systemSource = url }
    }

    private func chooseSystemTarget() {
        guard let url = OpenPanel.selectFile(
            title: ConverterCopy.text("Components.SystemTarget", language: language),
            message: ConverterCopy.text("Components.SystemTargetMessage", language: language)
        ) else { return }
        update { $0.systemTarget = url }
    }

    private func chooseExtraSource() {
        guard let url = OpenPanel.selectDirectory(
            title: ConverterCopy.text("Components.ExtDataSource", language: language),
            message: ConverterCopy.text("Components.ExtDataSourceMessage", language: language)
        ) else { return }
        update { $0.extraSourceDirectory = url }
    }

    private func chooseExtraStaging() {
        guard let url = OpenPanel.selectDirectory(
            title: ConverterCopy.text("Components.Staging", language: language),
            message: ConverterCopy.text("Components.StagingMessage", language: language)
        ) else { return }
        update { $0.extraStagingDirectory = url }
    }

    private func chooseExtraTarget() {
        guard let url = OpenPanel.selectDirectory(
            title: ConverterCopy.text("Components.Target", language: language),
            message: ConverterCopy.text("Components.TargetMessage", language: language)
        ) else { return }
        update { $0.extraTargetDirectory = url }
    }

    private func update(_ change: (inout ComponentSelection) -> Void) {
        var next = workflow.components
        change(&next)
        workflow.setComponents(next)
    }
}

private struct GroupScopeCaption: View {
    let image: String
    let names: String
    let enabled: Bool
    let language: ConverterLanguage

    var body: some View {
        Label(names, systemImage: image)
            .font(.caption.monospaced())
            .foregroundStyle(enabled ? .secondary : .tertiary)
            .padding(.leading, 6)
            .accessibilityLabel("\(ConverterCopy.text("Components.GroupScope", language: language)): \(names)")
    }
}
