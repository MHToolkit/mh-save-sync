using Microsoft.UI.Xaml;
using Microsoft.UI.Xaml.Controls;
using Microsoft.UI.Xaml.Media.Imaging;
using MHToolkit.MH3GSaveConverter.Windows.Models;

namespace MHToolkit.MH3GSaveConverter.Windows.Controls;

public sealed partial class StageArtwork : UserControl
{
    public static readonly DependencyProperty StageProperty = DependencyProperty.Register(
        nameof(Stage),
        typeof(WorkflowStage),
        typeof(StageArtwork),
        new PropertyMetadata(WorkflowStage.Input, OnStageChanged));

    public StageArtwork()
    {
        InitializeComponent();
        UpdateVisualState(WorkflowStage.Input);
    }

    public WorkflowStage Stage
    {
        get => (WorkflowStage)GetValue(StageProperty);
        set => SetValue(StageProperty, value);
    }

    private static void OnStageChanged(DependencyObject dependencyObject, DependencyPropertyChangedEventArgs args)
    {
        ((StageArtwork)dependencyObject).UpdateVisualState((WorkflowStage)args.NewValue);
    }

    private void UpdateVisualState(WorkflowStage stage)
    {
        var state = stage switch
        {
            WorkflowStage.Inspected => "Inspect",
            WorkflowStage.DryRunAuthorized => "DryRunAuthorized",
            WorkflowStage.Writing or WorkflowStage.Written or WorkflowStage.RolledBack => "Write",
            _ => "Input",
        };
        var artwork = stage switch
        {
            WorkflowStage.Inspected => "components-workshop.png",
            WorkflowStage.DryRunAuthorized => "dry-run-flow.png",
            WorkflowStage.Writing or WorkflowStage.Written or WorkflowStage.RolledBack => "rollback-harbor.png",
            _ => "input-route.png",
        };
        SceneImage.Source = new BitmapImage(new Uri($"ms-appx:///assets/Artwork/{artwork}"));
        VisualStateManager.GoToState(this, state, useTransitions: true);
    }
}
