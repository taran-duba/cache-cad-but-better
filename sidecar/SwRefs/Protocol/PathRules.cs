namespace SwRefs.Protocol;

/// <summary>
/// Path checks that behave the same on every OS, so Windows-style paths can be tested on macOS
/// (System.IO.Path treats "C:\..." as relative off Windows).
/// </summary>
public static class PathRules
{
    public static readonly IReadOnlyList<string> SupportedExtensions = [".sldasm", ".sldprt", ".slddrw"];

    /// <summary>File name after the last '\' or '/'.</summary>
    public static string FileName(string path)
    {
        var cut = path.LastIndexOfAny(['\\', '/']);
        return cut >= 0 ? path[(cut + 1)..] : path;
    }

    /// <summary>
    /// True for drive paths (C:\x, C:/x) and UNC paths (\\server\share) anywhere,
    /// and for /rooted paths when not running on Windows.
    /// </summary>
    public static bool IsAbsolute(string path)
    {
        if (path.Length >= 3 && char.IsAsciiLetter(path[0]) && path[1] == ':' && path[2] is '\\' or '/')
            return true;
        if (path.StartsWith(@"\\", StringComparison.Ordinal) || path.StartsWith("//", StringComparison.Ordinal))
            return path.Length > 2;
        return !OperatingSystem.IsWindows() && path.StartsWith('/');
    }

    public static bool HasSupportedExtension(string path)
    {
        var name = FileName(path);
        return SupportedExtensions.Any(ext => name.EndsWith(ext, StringComparison.OrdinalIgnoreCase) && name.Length > ext.Length);
    }
}
