#if SWDM
using System.Runtime.InteropServices;
using System.Runtime.Versioning;
using SolidWorks.Interop.swdocumentmgr;
using SwRefs.Protocol;

namespace SwRefs.Readers;

/// <summary>
/// Reads references with the SolidWorks Document Manager API, which parses the files directly
/// without a running SolidWorks. Only compiled when SWDM_INTEROP_PATH is set (Windows).
/// </summary>
[SupportedOSPlatform("windows")]
public sealed class SwdmReferenceReader(string? licenseKey, TextWriter log) : IReferenceReader
{
    private const string ClassFactoryProgId = "SwDocumentMgr.SwDMClassFactory";

    private SwDMApplication? _app;

    public string Name => "swdm";

    public IReadOnlyList<RefEntry> GetReferences(string path)
    {
        if (!File.Exists(path))
            throw new ReferenceReaderException(ErrorCodes.NotFound, $"file not found: {path}");

        var app = GetApplication();
        var doc = app.GetDocument(path, DocumentType(path), true, out SwDmDocumentOpenError openError);
        if (doc is null || openError != SwDmDocumentOpenError.swDmDocumentOpenErrorNone)
        {
            Release(doc);
            throw OpenError(openError, path);
        }

        try
        {
            // Only follow the path stored in the file: we want what the file says, not where
            // SolidWorks' search rules would happen to find a same-named file.
            var search = app.GetSearchOptionObject();
            search.SearchFilters = (int)SwDmSearchFilters.SwDmSearchExternalReference;

            var names = ToStrings(((SwDMDocument13)doc).GetAllExternalReferences4(
                search, out _, out object isVirtualVar, out _));
            var isVirtual = ToBools(isVirtualVar, names.Length);

            var refs = new List<RefEntry>(names.Length);
            for (var i = 0; i < names.Length; i++)
            {
                if (string.IsNullOrEmpty(names[i]))
                    continue;
                refs.Add(new RefEntry(names[i], File.Exists(names[i]), isVirtual[i]));
            }
            return refs;
        }
        finally
        {
            doc.CloseDoc();
            Release(doc);
        }
    }

    public void Dispose()
    {
        Release(_app);
        _app = null;
    }

    private SwDMApplication GetApplication()
    {
        if (_app is not null)
            return _app;

        if (string.IsNullOrWhiteSpace(licenseKey))
            throw new ReferenceReaderException(ErrorCodes.NoLicense, $"{Program.KeyEnvVar} is not set");

        var factoryType = Type.GetTypeFromProgID(ClassFactoryProgId)
            ?? throw new ReferenceReaderException(ErrorCodes.Unsupported,
                "SolidWorks Document Manager is not installed or registered on this machine");

        SwDMApplication? app;
        try
        {
            var factory = (SwDMClassFactory)Activator.CreateInstance(factoryType)!;
            app = factory.GetApplication(licenseKey);
        }
        catch (COMException e)
        {
            // Do not log the key itself.
            log.WriteLine($"swrefs: GetApplication failed: 0x{e.HResult:X8} {e.Message}");
            throw new ReferenceReaderException(ErrorCodes.NoLicense, "the Document Manager rejected the license key");
        }

        _app = app ?? throw new ReferenceReaderException(ErrorCodes.NoLicense, "the Document Manager rejected the license key");
        log.WriteLine("swrefs: Document Manager ready");
        return _app;
    }

    private static SwDmDocumentType DocumentType(string path)
    {
        var name = PathRules.FileName(path);
        if (name.EndsWith(".sldasm", StringComparison.OrdinalIgnoreCase)) return SwDmDocumentType.swDmDocumentAssembly;
        if (name.EndsWith(".sldprt", StringComparison.OrdinalIgnoreCase)) return SwDmDocumentType.swDmDocumentPart;
        if (name.EndsWith(".slddrw", StringComparison.OrdinalIgnoreCase)) return SwDmDocumentType.swDmDocumentDrawing;
        throw new ReferenceReaderException(ErrorCodes.Unsupported, $"not a SolidWorks part, assembly or drawing: {path}");
    }

    private static ReferenceReaderException OpenError(SwDmDocumentOpenError error, string path) => error switch
    {
        SwDmDocumentOpenError.swDmDocumentOpenErrorFileNotFound =>
            new(ErrorCodes.NotFound, $"file not found: {path}"),
        SwDmDocumentOpenError.swDmDocumentOpenErrorNoLicense =>
            new(ErrorCodes.NoLicense, "the Document Manager license key is not valid for this file"),
        SwDmDocumentOpenError.swDmDocumentOpenErrorNonSW =>
            new(ErrorCodes.OpenFailed, $"not a SolidWorks document: {path}"),
        SwDmDocumentOpenError.swDmDocumentOpenErrorFutureVersion =>
            new(ErrorCodes.OpenFailed, $"saved by a newer SolidWorks than this Document Manager: {path}"),
        _ => new(ErrorCodes.OpenFailed, $"could not open {path} ({error})"),
    };

    // The interop returns VARIANT arrays, which marshal as string[]/bool[] or object[].
    private static string[] ToStrings(object? value) => value switch
    {
        string[] s => s,
        object[] o => o.Select(x => x as string ?? "").ToArray(),
        _ => [],
    };

    private static bool[] ToBools(object? value, int length)
    {
        var result = new bool[length];
        switch (value)
        {
            case bool[] b:
                Array.Copy(b, result, Math.Min(b.Length, length));
                break;
            case object[] o:
                for (var i = 0; i < Math.Min(o.Length, length); i++)
                    result[i] = o[i] is true;
                break;
        }
        return result;
    }

    private static void Release(object? com)
    {
        if (com is not null && Marshal.IsComObject(com))
            Marshal.ReleaseComObject(com);
    }
}
#endif
