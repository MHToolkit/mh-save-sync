using System.Diagnostics;
using Microsoft.UI.Composition.SystemBackdrops;
using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media;
using Windows.Graphics;
using Windows.Storage.Pickers;
using WinRT.Interop;
using MHToolkit.MH3GSaveConverter.Windows.Models;
using MHToolkit.MH3GSaveConverter.Windows.Services;
using MHToolkit.MH3GSaveConverter.Windows.ViewModels;

namespace MHToolkit.MH3GSaveConverter.Windows;

public sealed partial class MainWindow : Window
{
    private readonly GitHubUpdateService _updateService = new();
    private readonly UpdateCheckPreferenceStore _updateCheckStore = new();
    private readonly SemaphoreSlim _dialogGate = new(1, 1);
    private Task<UpdateCheckResult>? _activeUpdateCheck;
    private bool _synchronizingLanguage;
    private bool _synchronizingConversionMode;
    private bool _synchronizingRepairVersion;
    private bool _loadedOnce;

    public MainWindow()
    {
        ViewModel = new MainViewModel();
        InitializeComponent();
        RootGrid.DataContext = ViewModel;
        ConfigureWindowMaterial();
        SelectLanguage(ViewModel.LanguageOverride);
        SelectConversionMode(ViewModel.SelectedConversionMode);
        SelectRepairVersion(null);
    }

    public MainViewModel ViewModel { get; }

    private async void RootGrid_Loaded(object sender, RoutedEventArgs e)
    {
        if (_loadedOnce)
        {
            return;
        }
        _loadedOnce = true;

        if (!_updateCheckStore.ShouldCheckToday())
        {
            return;
        }

        // Record the attempt before networking so a blocked GitHub connection
        // is not retried on every launch during the same local calendar day.
        _updateCheckStore.MarkAttempt();
        await CheckForUpdatesAsync(manual: false);
    }

    private async void About_Click(object sender, RoutedEventArgs e)
    {
        var content = new StackPanel { Spacing = 10, Width = 520 };
        content.Children.Add(new TextBlock
        {
            Text = ViewModel.Copy.AboutDescription,
            TextWrapping = TextWrapping.Wrap,
        });
        content.Children.Add(new TextBlock
        {
            Text = $"{ViewModel.Copy.CurrentVersion}: {_updateService.CurrentVersion}",
            FontFamily = new FontFamily("Cascadia Mono"),
        });
        content.Children.Add(new TextBlock
        {
            Text = ViewModel.Copy.UpdateNetworkNote,
            TextWrapping = TextWrapping.Wrap,
            Foreground = (Brush)Application.Current.Resources["ConverterSlateBrush"],
        });

        var dialog = new ContentDialog
        {
            Title = ViewModel.Copy.AboutUpdates,
            Content = content,
            PrimaryButtonText = ViewModel.Copy.CheckForUpdates,
            CloseButtonText = ViewModel.Copy.Close,
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await ShowDialogAsync(dialog) == ContentDialogResult.Primary)
        {
            await CheckForUpdatesAsync(manual: true);
        }
    }

    private async Task CheckForUpdatesAsync(bool manual)
    {
        try
        {
            var activeCheck = _activeUpdateCheck;
            if (activeCheck is null || activeCheck.IsCompleted)
            {
                activeCheck = _updateService.CheckAsync();
                _activeUpdateCheck = activeCheck;
            }
            var result = await activeCheck;
            if (result.IsUpdateAvailable)
            {
                await ShowUpdateAvailableAsync(result);
            }
            else if (manual)
            {
                await ShowMessageAsync(
                    ViewModel.Copy.UpToDateTitle,
                    string.Format(ViewModel.Copy.UpToDateMessage, result.Release.TagName));
            }
        }
        catch (Exception exception)
        {
            // Automatic checks are advisory and silent. They never delay the
            // local workflow beyond their own timeout or surface a startup
            // blocker when GitHub is unavailable in the user's network.
            if (manual)
            {
                await ShowMessageAsync(
                    ViewModel.Copy.UpdateCheckFailed,
                    $"{ViewModel.Copy.UpdateNetworkNote}\n\n{exception.Message}");
            }
        }
    }

    private async Task ShowUpdateAvailableAsync(UpdateCheckResult result)
    {
        var release = result.Release;
        var content = new StackPanel { Spacing = 10, Width = 560 };
        content.Children.Add(new TextBlock
        {
            Text = string.Format(
                ViewModel.Copy.UpdateAvailableSummary,
                result.CurrentVersion,
                release.TagName),
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        if (release.PublishedAt is { } publishedAt)
        {
            content.Children.Add(new TextBlock
            {
                Text = publishedAt.ToLocalTime().ToString("D"),
                Foreground = (Brush)Application.Current.Resources["ConverterSlateBrush"],
            });
        }
        content.Children.Add(new TextBlock
        {
            Text = ViewModel.Copy.ReleaseNotes,
            FontWeight = Microsoft.UI.Text.FontWeights.SemiBold,
        });
        content.Children.Add(new ScrollViewer
        {
            MaxHeight = 320,
            VerticalScrollBarVisibility = ScrollBarVisibility.Auto,
            Content = new TextBlock
            {
                Text = string.IsNullOrWhiteSpace(release.Body) ? ViewModel.Copy.NoReleaseNotes : release.Body,
                TextWrapping = TextWrapping.Wrap,
                IsTextSelectionEnabled = true,
            },
        });

        var dialog = new ContentDialog
        {
            Title = string.IsNullOrWhiteSpace(release.Name)
                ? ViewModel.Copy.UpdateAvailableTitle
                : release.Name,
            Content = content,
            PrimaryButtonText = ViewModel.Copy.OpenReleasePage,
            CloseButtonText = ViewModel.Copy.Close,
            DefaultButton = ContentDialogButton.Primary,
        };
        if (await ShowDialogAsync(dialog) == ContentDialogResult.Primary)
        {
            Process.Start(new ProcessStartInfo(release.HtmlUrl) { UseShellExecute = true });
        }
    }

    private async Task ShowMessageAsync(string title, string message)
    {
        var dialog = new ContentDialog
        {
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap },
            CloseButtonText = ViewModel.Copy.Close,
            DefaultButton = ContentDialogButton.Close,
        };
        await ShowDialogAsync(dialog);
    }

    private async Task<ContentDialogResult> ShowDialogAsync(ContentDialog dialog)
    {
        await _dialogGate.WaitAsync();
        try
        {
            dialog.XamlRoot = RootGrid.XamlRoot;
            return await dialog.ShowAsync();
        }
        finally
        {
            _dialogGate.Release();
        }
    }

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

    private void SelectConversionMode(Models.ConversionMode mode)
    {
        _synchronizingConversionMode = true;
        ConversionModePicker.SelectedIndex = mode == Models.ConversionMode.RepairConverted ? 1 : 0;
        _synchronizingConversionMode = false;
    }

    private void ConversionModePicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_synchronizingConversionMode
            || ConversionModePicker.SelectedItem is not ComboBoxItem item)
        {
            return;
        }

        ViewModel.SetConversionMode(item.Tag as string);
        SelectRepairVersion(null);
    }

    private void SelectRepairVersion(string? version)
    {
        _synchronizingRepairVersion = true;
        RepairVersionPicker.SelectedIndex = version switch
        {
            "0.0.3" => 1,
            "0.0.4" => 2,
            "0.0.5" => 3,
            "0.0.6" => 4,
            _ => 0,
        };
        _synchronizingRepairVersion = false;
    }

    private void RepairVersionPicker_SelectionChanged(object sender, SelectionChangedEventArgs e)
    {
        if (_synchronizingRepairVersion
            || RepairVersionPicker.SelectedItem is not ComboBoxItem item)
        {
            return;
        }

        ViewModel.SetRepairFromVersion(item.Tag as string);
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
        if (ReferenceEquals(sender, SourcePathBox))
        {
            ViewModel.SourcePath = SourcePathBox.Text;
        }
        else if (ReferenceEquals(sender, TargetPathBox))
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

        var slot = (Path.GetFileName(path) ?? string.Empty).ToLowerInvariant();
        if (slot is "user1" or "user2" or "user3")
        {
            ViewModel.SelectedSlot = slot;
        }
    }

    private void CecPath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (ReferenceEquals(sender, CecSourceBox))
        {
            ViewModel.CecSourceDirectory = CecSourceBox.Text;
        }
        else if (ReferenceEquals(sender, CecTargetBox))
        {
            ViewModel.CecTargetPath = CecTargetBox.Text;
        }
    }

    private void SystemPath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (ReferenceEquals(sender, SystemSourceBox))
        {
            ViewModel.SystemSourcePath = SystemSourceBox.Text;
        }
        else if (ReferenceEquals(sender, SystemTargetBox))
        {
            ViewModel.SystemTargetPath = SystemTargetBox.Text;
        }
    }

    private void ExtrasPath_TextChanged(object sender, TextChangedEventArgs e)
    {
        if (ReferenceEquals(sender, ExtrasSourceBox))
        {
            ViewModel.ExtrasSourceDirectory = ExtrasSourceBox.Text;
        }
        else if (ReferenceEquals(sender, ExtrasStagingBox))
        {
            ViewModel.ExtrasStagingDirectory = ExtrasStagingBox.Text;
        }
        else if (ReferenceEquals(sender, ExtrasTargetBox))
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
            Title = title,
            Content = new TextBlock { Text = message, TextWrapping = TextWrapping.Wrap },
            PrimaryButtonText = ViewModel.Copy.Continue,
            CloseButtonText = ViewModel.Copy.Cancel,
            DefaultButton = ContentDialogButton.Close,
        };
        return await ShowDialogAsync(dialog) == ContentDialogResult.Primary;
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
