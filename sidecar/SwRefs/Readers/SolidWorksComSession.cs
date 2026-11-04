using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using SwRefs.Protocol;

namespace SwRefs.Readers;

/// <summary>
/// Late-bound COM connection to SolidWorks (no interop DLL, so nothing to redistribute and no
/// SolidWorks needed to build). Attaches to a running SolidWorks if there is one; otherwise starts
/// a hidden one and closes it again on dispose. A SolidWorks that was already running is left alone.
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class SolidWorksComSession : ISolidWorksSession
{
    private const string ProgId = "SldWorks.Application";

    private object? _app;
    private readonly bool _startedByUs;
    private readonly TextWriter _log;

    private SolidWorksComSession(object app, bool startedByUs, TextWriter log)
    {
        _app = app;
        _startedByUs = startedByUs;
        _log = log;
    }

    public static ISolidWorksSession Connect(TextWriter log)
    {
        var type = Type.GetTypeFromProgID(ProgId)
            ?? throw new ReferenceReaderException(ErrorCodes.Unsupported,
                "SolidWorks is not installed on this computer, so references can't be read. Install it, or add a Document Manager key in Settings.");

        if (TryGetRunning(type.GUID) is { } running)
        {
            log.WriteLine("swrefs: attached to the running SolidWorks");
            return new SolidWorksComSession(running, startedByUs: false, log);
        }

        log.WriteLine("swrefs: starting a hidden SolidWorks (this can take a minute)");
        object app;
        try
        {
            app = Activator.CreateInstance(type)!;
            ((dynamic)app).Visible = false;
        }
        catch (COMException e)
        {
            throw new ReferenceReaderException(ErrorCodes.OpenFailed,
                $"could not start SolidWorks (0x{e.HResult:X8}): {e.Message}");
        }
        return new SolidWorksComSession(app, startedByUs: true, log);
    }

    // TYPE_E_ELEMENTNOTFOUND / DISP_E_UNKNOWNNAME: the dynamic binder couldn't resolve the member
    // through SolidWorks' type information (seen with an unregistered or mismatched type library).
    private static readonly int[] BinderFailures = [unchecked((int)0x8002802B), unchecked((int)0x80020006)];

    public object? GetDocumentDependencies(string path)
    {
        var app = _app ?? throw new ObjectDisposedException(nameof(SolidWorksComSession));
        try
        {
            return ((dynamic)app).GetDocumentDependencies2(path, false, false, false);
        }
        catch (COMException e) when (BinderFailures.Contains(e.HResult))
        {
            // Call IDispatch by name directly, which doesn't depend on the type library.
            _log.WriteLine($"swrefs: dynamic call failed (0x{e.HResult:X8}); retrying through IDispatch");
            try
            {
                return app.GetType().InvokeMember("GetDocumentDependencies2",
                    System.Reflection.BindingFlags.InvokeMethod, null, app, [path, false, false, false]);
            }
            catch (System.Reflection.TargetInvocationException t) when (t.InnerException is COMException inner)
            {
                throw inner;
            }
        }
    }

    public void Dispose()
    {
        var app = _app;
        _app = null;
        if (app is null)
            return;
        try
        {
            if (_startedByUs)
                ((dynamic)app).ExitApp();
        }
        catch (COMException e)
        {
            _log.WriteLine($"swrefs: closing SolidWorks failed: 0x{e.HResult:X8} {e.Message}");
        }
        finally
        {
            if (Marshal.IsComObject(app))
                Marshal.ReleaseComObject(app);
        }
    }

    private static object? TryGetRunning(Guid clsid)
    {
        var id = clsid;
        return GetActiveObject(ref id, IntPtr.Zero, out var obj) == 0 ? obj : null;
    }

    [DllImport("oleaut32.dll")]
    private static extern int GetActiveObject(ref Guid rclsid, IntPtr reserved,
        [MarshalAs(UnmanagedType.IUnknown)] out object? obj);
}
