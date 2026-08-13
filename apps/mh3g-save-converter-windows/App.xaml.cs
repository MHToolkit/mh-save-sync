using Microsoft.UI.Xaml;

namespace MHToolkit.MH3GSaveConverter.Windows;

public partial class App : Application
{
    private Window? _window;

    public App()
    {
        InitializeComponent();
    }

    protected override void OnLaunched(LaunchActivatedEventArgs args)
    {
        var fixture = ParseFixture(Environment.GetCommandLineArgs())
            ?? ParseFixtureEnvironment(Environment.GetEnvironmentVariable("MH3G_UI_FIXTURE"));
        var viewModel = new ViewModels.MainViewModel();
        if (fixture is not null)
        {
            viewModel.ApplySyntheticFixture(fixture);
        }
        _window = new MainWindow(viewModel, fixture);
        _window.Activate();
    }

    private static string? ParseFixture(string[] arguments)
    {
        for (var index = 0; index < arguments.Length; index++)
        {
            if (!string.Equals(arguments[index], "--ui-fixture", StringComparison.Ordinal))
            {
                continue;
            }
            if (index + 1 >= arguments.Length)
            {
                throw new ArgumentException("--ui-fixture requires an allowlisted fixture id.");
            }
            return arguments[index + 1];
        }
        return null;
    }

    private static string? ParseFixtureEnvironment(string? value) =>
        string.IsNullOrWhiteSpace(value) ? null : value.Trim();
}
