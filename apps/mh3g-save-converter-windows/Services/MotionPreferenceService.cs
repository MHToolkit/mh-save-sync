using Windows.UI.ViewManagement;

namespace MHToolkit.MH3GSaveConverter.Windows.Services;

/// <summary>
/// Reads the Windows animation preference immediately before a causal UI
/// transition. The state change never depends on animation availability.
/// </summary>
public sealed class MotionPreferenceService
{
    public bool AnimationsEnabled
    {
        get
        {
            try
            {
                return new UISettings().AnimationsEnabled;
            }
            catch
            {
                return false;
            }
        }
    }
}
