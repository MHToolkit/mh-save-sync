using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Windows.Graphics;
using Windows.Storage.Pickers;
using WinRT.Interop;
using MHToolkit.MH3GSaveConverter.Windows.ViewModels;

namespace MHToolkit.MH3GSaveConverter.Windows;

public sealed partial class MainWindow : Window
{
    private bool _synchronizingLanguage;

    public MainWindow()
    {
        ViewModel = new MainViewModel();
        InitializeComponent();
        DataContext = ViewModel;
        ConfigureWindowMaterial();
        SelectLanguage(ViewModel.LanguageOverride);
    }

    public MainViewModel ViewModel { get; }

    private void ConfigureWindowMaterial()
    {
        AppWindow.Resize(new SizeInt32(1240, 900));
        if (MicaController.IsSupported())
        {
            SystemBackdrop = new MicaBackdrop();
        }
    }

    private void SelectLanguage(Models.AppLanguageOverride language)
    {
        _synchronizingLanguage = true;
        LanguagePicker.SelectedIndex = language switch
        {
            Models.AppLanguageOverride.Chinese => 1,
            Models.AppLanguageOverride.English => 2,
            _ => 0,
        };
        _synchronizingLanguage = false;
    }

    private void LanguagePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_synchronizingLanguage || LanguagePicker.SelectedItem is not ComboBoxItem item)
        {
            return;
        }

        ViewModel.SetLanguage(item.Tag as string);
    }

    private async void ChooseSourceFile_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync("*");
        if (path is not null)
        {
            ApplySourceSelection(path);
        }
    }

    private async void ChooseSourceFolder_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFolderAsync();
        if (path is not null)
        {
            SourcePathBox.Text = path;
            ViewModel.SourcePath = path;
        }
    }

    private async void ChooseTargetFile_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync("*");
        if (path is not null)
        {
            TargetPathBox.Text = path;
            ViewModel.TargetPath = path;
        }
    }

    private async void ChooseTargetFolder_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFolderAsync();
        if (path is not null)
        {
            TargetPathBox.Text = path;
            ViewModel.TargetPath = path;
        }
    }

    private async void ChooseCli_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".exe");
        if (path is not null)
        {
            CliPathBox.Text = path;
            ViewModel.CliPath = path;
        }
    }

    private async void ChooseRollbackManifest_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".json");
        if (path is not null)
        {
            RollbackManifestBox.Text = path;
            ViewModel.RollbackManifestPath = path;
        }
    }

    private async void ChooseSystemSource_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync("*");
        if (path is not null)
        {
            SystemSourceBox.Text = path;
            ViewModel.SystemSourcePath = path;
        }
    }

    private async void ChooseSystemTarget_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync("*");
        if (path is not null)
        {
            SystemTargetBox.Text = path;
            ViewModel.SystemTargetPath = path;
        }
    }

    private async void ChooseSystemRollbackManifest_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".json");
        if (path is not null)
        {
            SystemRollbackManifestBox.Text = path;
            ViewModel.SystemRollbackManifestPath = path;
        }
    }

    private async void ChooseExtrasSource_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFolderAsync();
        if (path is not null)
        {
            ExtrasSourceBox.Text = path;
            ViewModel.ExtrasSourceDirectory = path;
        }
    }

    private async void ChooseExtrasStaging_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFolderAsync();
        if (path is not null)
        {
            ExtrasStagingBox.Text = path;
            ViewModel.ExtrasStagingDirectory = path;
        }
    }

    private async void ChooseExtrasTarget_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFolderAsync();
        if (path is not null)
        {
            ExtrasTargetBox.Text = path;
            ViewModel.ExtrasTargetDirectory = path;
        }
    }

    private async void ChooseExtrasRollbackManifest_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".json");
        if (path is not null)
        {
            ExtrasRollbackManifestBox.Text = path;
            ViewModel.ExtrasRollbackManifestPath = path;
        }
    }

    private async void ChooseCecDirectory_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFolderAsync();
        if (path is not null)
        {
            CecSourceBox.Text = path;
            ViewModel.CecSourceDirectory = path;
        }
    }

    private async void ChooseCecTarget_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync("*");
        if (path is not null)
        {
            CecTargetBox.Text = path;
            ViewModel.CecTargetPath = path;
        }
    }

    private async void ChooseCecRollbackManifest_Click(object sender, RoutedEventArgs e)
    {
        var path = await PickFileAsync(".json");
        if (path is not null)
        {
            CecRollbackManifestBox.Text = path;
            ViewModel.CecRollbackManifestPath = path;
        }
    }

    private void CorePath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (sender == SourcePathBox)
        {
            ViewModel.SourcePath = SourcePathBox.Text;
        }
        else if (sender == TargetPathBox)
        {
            ViewModel.TargetPath = TargetPathBox.Text;
        }
    }

    private void GoToCoreWorkflow_Click(object sender, RoutedEventArgs e)
    {
        CoreWorkflowAnchor.StartBringIntoView();
    }

    private void GoToOptionalConfiguration_Click(object sender, RoutedEventArgs e)
    {
        OptionalConfigurationAnchor.StartBringIntoView();
    }

    private void GoToPostWriteDestination_Click(object sender, RoutedEventArgs e)
    {
        if (ViewModel.HasPendingSelectedOptionalWork)
        {
            OptionalConfigurationAnchor.StartBringIntoView();
            return;
        }

        ResultAnchor.StartBringIntoView();
    }

    private void GoToResult_Click(object sender, RoutedEventArgs e)
    {
        ResultAnchor.StartBringIntoView();
    }

    private void ApplySourceSelection(string path)
    {
        SourcePathBox.Text = path;
        ViewModel.SourcePath = path;

        var slot = Path.GetFileName(path).ToLowerInvariant();
        if (slot is "user1" or "user2" or "user3")
        {
            ViewModel.SelectedSlot = slot;
        }
    }

    private void CecPath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (sender == CecSourceBox)
        {
            ViewModel.CecSourceDirectory = CecSourceBox.Text;
        }
        else if (sender == CecTargetBox)
        {
            ViewModel.CecTargetPath = CecTargetBox.Text;
        }
    }

    private void SystemPath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (sender == SystemSourceBox)
        {
            ViewModel.SystemSourcePath = SystemSourceBox.Text;
        }
        else if (sender == SystemTargetBox)
        {
            ViewModel.SystemTargetPath = SystemTargetBox.Text;
        }
    }

    private void ExtrasPath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (sender == ExtrasSourceBox)
        {
            ViewModel.ExtrasSourceDirectory = ExtrasSourceBox.Text;
        }
        else if (sender == ExtrasStagingBox)
        {
            ViewModel.ExtrasStagingDirectory = ExtrasStagingBox.Text;
        }
        else if (sender == ExtrasTargetBox)
        {
            ViewModel.ExtrasTargetDirectory = ExtrasTargetBox.Text;
        }
    }

    private async void Inspect_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.InspectCoreAsync);
    private async void InspectProgress_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.InspectProgressAsync);
    private async void InspectEvents_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.InspectEventsAsync);
    private async void DryRun_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.RunCoreDryRunAsync);
    private async void SystemDryRun_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.RunSystemDryRunAsync);
    private async void ExtrasStageDryRun_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.RunExtrasStageDryRunAsync);
    private async void ExtrasInstallDryRun_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.RunExtrasInstallDryRunAsync);
    private async void InspectCec_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.InspectCecAsync);
    private async void CecDryRun_Click(object sender, RoutedEventArgs e) => await RunSafelyAsync(ViewModel.RunCecDryRunAsync);

    private async void WriteCore_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmWriteTitle, ViewModel.Copy.ConfirmWriteBody))
        {
            await RunSafelyAsync(ViewModel.WriteCoreAsync);
        }
    }

    private async void RollbackCore_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmRollbackTitle, ViewModel.Copy.ConfirmRollbackBody))
        {
            await RunSafelyAsync(ViewModel.RollbackCoreAsync);
        }
    }

    private async void WriteSystem_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmWriteTitle, ViewModel.Copy.ConfirmWriteBody))
        {
            await RunSafelyAsync(ViewModel.WriteSystemAsync);
        }
    }

    private async void RollbackSystem_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmRollbackTitle, ViewModel.Copy.ConfirmRollbackBody))
        {
            await RunSafelyAsync(ViewModel.RollbackSystemAsync);
        }
    }

    private async void StageExtras_Click(object sender, RoutedEventArgs e)
    {
        await RunSafelyAsync(ViewModel.StageExtrasAsync);
    }

    private async void InstallExtras_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmWriteTitle, ViewModel.Copy.ConfirmWriteBody))
        {
            await RunSafelyAsync(ViewModel.InstallExtrasAsync);
        }
    }

    private async void RollbackExtras_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmRollbackTitle, ViewModel.Copy.ConfirmRollbackBody))
        {
            await RunSafelyAsync(ViewModel.RollbackExtrasAsync);
        }
    }

    private async void WriteCec_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmCecTitle, ViewModel.Copy.ConfirmCecBody))
        {
            await RunSafelyAsync(ViewModel.WriteCecAsync);
        }
    }

    private async void RollbackCec_Click(object sender, RoutedEventArgs e)
    {
        if (await ConfirmAsync(ViewModel.Copy.ConfirmRollbackTitle, ViewModel.Copy.ConfirmRollbackBody))
        {
            await RunSafelyAsync(ViewModel.RollbackCecAsync);
        }
    }

    private void CecToggle_Toggled(object sender, RoutedEventArgs e)
    {
        ViewModel.IsCecEnabled = CecToggle.IsOn;
        CecControls.Visibility = CecToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
    }

    private void SystemToggle_Toggled(object sender, RoutedEventArgs e)
    {
        ViewModel.IsSystemEnabled = SystemToggle.IsOn;
        SystemControls.Visibility = SystemToggle.IsOn ? Visibility.Visible : Visibility.Collapsed;
    }

    private async Task<string?> PickFileAsync(params string[] extensions)
    {
        var picker = new FileOpenPicker();
        foreach (var extension in extensions)
        {
            picker.FileTypeFilter.Add(extension);
        }
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var file = await picker.PickSingleFileAsync();
        return file?.Path;
    }

    private async Task<string?> PickFolderAsync()
    {
        var picker = new FolderPicker();
        picker.FileTypeFilter.Add("*");
        InitializeWithWindow.Initialize(picker, WindowNative.GetWindowHandle(this));
        var folder = await picker.PickSingleFolderAsync();
        return folder?.Path;
    }

    private async Task<bool> ConfirmAsync(string title, string message)
    {
        var dialog = new ContentDialog
        {
            XamlRoot = RootGrid.XamlRoot,
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap },
            PrimaryButtonText = ViewModel.Copy.Continue,
            CloseButtonText = ViewModel.Copy.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        return await dialog.ShowAsync() == ContentDialogResult.Primary;
    }

    private async Task RunSafelyAsync(Func<Task> operation)
    {
        try
        {
            await operation();
        }
        catch (Exception exception)
        {
            ViewModel.ReportUnexpected(exception);
        }
    }
}
