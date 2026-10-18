using System.Reflection;

namespace SwRefs.Protocol;

public static class ProtocolInfo
{
    /// <summary>Semver reported by "ping"; comes from &lt;Version&gt; in SwRefs.csproj.</summary>
    public static readonly string Version = ReadVersion();

    private static string ReadVersion()
    {
        var info = typeof(ProtocolInfo).Assembly.GetCustomAttribute<AssemblyInformationalVersionAttribute>()?.InformationalVersion;
        if (string.IsNullOrEmpty(info))
            return "0.0.0";
        var plus = info.IndexOf('+');
        return plus >= 0 ? info[..plus] : info;
    }
}
