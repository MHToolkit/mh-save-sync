namespace MHToolkit.MH3GSaveConverter.Windows.Models;

/// <summary>
/// The exact core paths derived from one explicit user selection. Directory
/// selections are convenience only: they always resolve to one direct child
/// and are never searched recursively.
/// </summary>
public sealed record CoreSavePaths(string Source, string Target, string Slot, string? Current = null);

public enum SavePathResolutionError
{
    MissingSourceSelection,
    MissingTargetSelection,
    MissingCurrentSelection,
    InvalidSlot,
    SourcePathMissing,
    SourceSlotMissing,
    SourceSlotNameMismatch,
    TargetPathMissing,
    TargetSlotNameMismatch,
    CurrentPathMissing,
    CurrentSlotMissing,
    CurrentSlotNameMismatch,
    ExtDataPathMissing,
    ExtDataUserDirectoryMissing,
    InvalidPath,
}

/// <summary>
/// Resolves only user1/user2/user3 selections and the narrow ExtData `user`
/// directory convention. It deliberately never discovers an SD card, MLC
/// root, archive, or any recursively nested save file.
/// </summary>
public static class SavePathResolver
{
    private static readonly HashSet<string> Slots = new(StringComparer.OrdinalIgnoreCase)
    {
        "user1",
        "user2",
        "user3",
    };

    public static IReadOnlyList<string> AvailableSlots { get; } = new[] { "user1", "user2", "user3" };

    public static bool TryResolveCore(
        string sourceSelection,
        string targetSelection,
        string slot,
        out CoreSavePaths? paths,
        out SavePathResolutionError error)
    {
        paths = null;
        if (!IsSlot(slot))
        {
            error = SavePathResolutionError.InvalidSlot;
            return false;
        }

        if (!TryResolveSource(sourceSelection, slot, out var source, out error)
            || !TryResolveTarget(targetSelection, slot, out var target, out error))
        {
            return false;
        }

        paths = new CoreSavePaths(source, target, slot);
        return true;
    }

    public static bool TryResolveRepairCore(
        string sourceSelection,
        string currentSelection,
        string targetSelection,
        string slot,
        out CoreSavePaths? paths,
        out SavePathResolutionError error)
    {
        paths = null;
        if (!IsSlot(slot))
        {
            error = SavePathResolutionError.InvalidSlot;
            return false;
        }

        if (!TryResolveSource(sourceSelection, slot, out var source, out error)
            || !TryResolveCurrent(currentSelection, slot, out var current, out error)
            || !TryResolveTarget(targetSelection, slot, out var target, out error))
        {
            return false;
        }

        paths = new CoreSavePaths(source, target, slot, current);
        return true;
    }

    public static bool TryResolveSource(
        string selection,
        string slot,
        out string resolved,
        out SavePathResolutionError error)
    {
        resolved = string.Empty;
        if (string.IsNullOrWhiteSpace(selection))
        {
            error = SavePathResolutionError.MissingSourceSelection;
            return false;
        }

        if (!IsSlot(slot))
        {
            error = SavePathResolutionError.InvalidSlot;
            return false;
        }

        if (!TryGetFullPath(selection, out var fullPath))
        {
            error = SavePathResolutionError.InvalidPath;
            return false;
        }

        if (Directory.Exists(fullPath))
        {
            var candidate = Path.Combine(fullPath, slot);
            if (!File.Exists(candidate))
            {
                error = SavePathResolutionError.SourceSlotMissing;
                return false;
            }

            resolved = candidate;
            error = default;
            return true;
        }

        if (!File.Exists(fullPath))
        {
            error = SavePathResolutionError.SourcePathMissing;
            return false;
        }

        if (!string.Equals(Path.GetFileName(fullPath), slot, StringComparison.OrdinalIgnoreCase))
        {
            error = SavePathResolutionError.SourceSlotNameMismatch;
            return false;
        }

        resolved = fullPath;
        error = default;
        return true;
    }

    public static bool TryResolveTarget(
        string selection,
        string slot,
        out string resolved,
        out SavePathResolutionError error)
    {
        resolved = string.Empty;
        if (string.IsNullOrWhiteSpace(selection))
        {
            error = SavePathResolutionError.MissingTargetSelection;
            return false;
        }

        if (!IsSlot(slot))
        {
            error = SavePathResolutionError.InvalidSlot;
            return false;
        }

        if (!TryGetFullPath(selection, out var fullPath))
        {
            error = SavePathResolutionError.InvalidPath;
            return false;
        }

        if (Directory.Exists(fullPath))
        {
            // This is intentionally only a derived path. No file is created
            // here; the Rust transaction is the sole writer.
            resolved = Path.Combine(fullPath, slot);
            error = default;
            return true;
        }

        if (!File.Exists(fullPath))
        {
            error = SavePathResolutionError.TargetPathMissing;
            return false;
        }

        if (!string.Equals(Path.GetFileName(fullPath), slot, StringComparison.OrdinalIgnoreCase))
        {
            error = SavePathResolutionError.TargetSlotNameMismatch;
            return false;
        }

        resolved = fullPath;
        error = default;
        return true;
    }

    public static bool TryResolveCurrent(
        string selection,
        string slot,
        out string resolved,
        out SavePathResolutionError error)
    {
        resolved = string.Empty;
        if (string.IsNullOrWhiteSpace(selection))
        {
            error = SavePathResolutionError.MissingCurrentSelection;
            return false;
        }

        if (!IsSlot(slot))
        {
            error = SavePathResolutionError.InvalidSlot;
            return false;
        }

        if (!TryGetFullPath(selection, out var fullPath))
        {
            error = SavePathResolutionError.InvalidPath;
            return false;
        }

        if (Directory.Exists(fullPath))
        {
            var candidate = Path.Combine(fullPath, slot);
            if (!File.Exists(candidate))
            {
                error = SavePathResolutionError.CurrentSlotMissing;
                return false;
            }

            resolved = candidate;
            error = default;
            return true;
        }

        if (!File.Exists(fullPath))
        {
            error = SavePathResolutionError.CurrentPathMissing;
            return false;
        }

        if (!string.Equals(Path.GetFileName(fullPath), slot, StringComparison.OrdinalIgnoreCase))
        {
            error = SavePathResolutionError.CurrentSlotNameMismatch;
            return false;
        }

        resolved = fullPath;
        error = default;
        return true;
    }

    public static bool TryResolveExtDataUserDirectory(
        string selection,
        out string resolved,
        out SavePathResolutionError error)
    {
        resolved = string.Empty;
        if (string.IsNullOrWhiteSpace(selection))
        {
            error = SavePathResolutionError.ExtDataPathMissing;
            return false;
        }

        if (!TryGetFullPath(selection, out var fullPath))
        {
            error = SavePathResolutionError.InvalidPath;
            return false;
        }

        if (!Directory.Exists(fullPath))
        {
            error = SavePathResolutionError.ExtDataPathMissing;
            return false;
        }

        if (string.Equals(Path.GetFileName(fullPath), "user", StringComparison.OrdinalIgnoreCase))
        {
            resolved = fullPath;
            error = default;
            return true;
        }

        var candidate = Path.Combine(fullPath, "user");
        if (!Directory.Exists(candidate))
        {
            error = SavePathResolutionError.ExtDataUserDirectoryMissing;
            return false;
        }

        resolved = candidate;
        error = default;
        return true;
    }

    public static bool IsSlot(string? value) => value is not null && Slots.Contains(value);

    private static bool TryGetFullPath(string path, out string fullPath)
    {
        try
        {
            fullPath = Path.GetFullPath(path);
            return true;
        }
        catch (Exception exception) when (exception is ArgumentException or NotSupportedException or PathTooLongException)
        {
            fullPath = string.Empty;
            return false;
        }
    }
}
