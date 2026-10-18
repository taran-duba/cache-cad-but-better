using System.Text;
using SwRefs.Protocol;
using SwRefs.Readers;

namespace SwRefs;

public static class Program
{
    /// <summary>Path to a fake-reader fixture; when set, the Document Manager is not used.</summary>
    public const string FakeEnvVar = "CADSYNC_SWREFS_FAKE";
    /// <summary>SolidWorks Document Manager license key. Never hard-code or log it.</summary>
    public const string KeyEnvVar = "CADSYNC_SWDM_KEY";

    // The Document Manager is a COM server; an STA main thread matches how its samples call it.
    [STAThread]
    public static int Main()
    {
        var utf8 = new UTF8Encoding(encoderShouldEmitUTF8Identifier: false);
        var stdout = new StreamWriter(Console.OpenStandardOutput(), utf8) { AutoFlush = false };
        var stdin = new StreamReader(Console.OpenStandardInput(), utf8);
        var log = Console.Error;
        // Stdout carries responses only: anything else that writes to Console.Out goes to stderr.
        Console.SetOut(log);

        using var reader = CreateReader(Environment.GetEnvironmentVariable, log);
        log.WriteLine($"swrefs {ProtocolInfo.Version} ready (reader: {reader.Name})");
        Server.Run(stdin, stdout, new RequestHandler(reader, log));
        return 0;
    }

    public static IReferenceReader CreateReader(Func<string, string?> env, TextWriter log)
    {
        var fixture = env(FakeEnvVar);
        if (!string.IsNullOrWhiteSpace(fixture))
        {
            try
            {
                return FakeReferenceReader.FromFile(fixture);
            }
            catch (Exception e) when (e is IOException or UnauthorizedAccessException or FormatException)
            {
                var message = $"fake fixture {fixture} could not be loaded: {e.Message}";
                log.WriteLine($"swrefs: {message}");
                return new UnavailableReferenceReader(ErrorCodes.OpenFailed, message);
            }
        }

#if SWDM
        if (OperatingSystem.IsWindows())
            return new SwdmReferenceReader(env(KeyEnvVar), log);
        return new UnavailableReferenceReader(ErrorCodes.Unsupported, "the SolidWorks Document Manager only runs on Windows");
#else
        return new UnavailableReferenceReader(ErrorCodes.Unsupported,
            $"this build has no SolidWorks Document Manager support; rebuild on Windows with SWDM_INTEROP_PATH set, or set {FakeEnvVar}");
#endif
    }
}
