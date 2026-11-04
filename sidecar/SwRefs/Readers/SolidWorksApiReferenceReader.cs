using SwRefs.Protocol;

namespace SwRefs.Readers;

/// <summary>A connection to SolidWorks' own automation API.</summary>
public interface ISolidWorksSession : IDisposable
{
    /// <summary>
    /// <c>ISldWorks.GetDocumentDependencies2(path, traverse: false, search: false, addReadOnlyInfo: false)</c>:
    /// the file's direct references, as the raw array SolidWorks returns.
    /// </summary>
    object? GetDocumentDependencies(string path);

    /// <summary>
    /// <c>ISldWorks.ReplaceReferencedDocument(referencing, referenced, newReference)</c>: repoints a
    /// reference inside a closed document. Returns SolidWorks' success flag.
    /// </summary>
    bool ReplaceReferencedDocument(string referencing, string referenced, string newReference) =>
        throw new NotSupportedException();
}

/// <summary>
/// Reads references by asking a running (or hidden, freshly started) SolidWorks. Needs no
/// Document Manager key, but SolidWorks must be installed on this PC. SolidWorks is only contacted
/// on the first <c>refs</c> request, so <c>ping</c> works on machines without it.
/// </summary>
public sealed class SolidWorksApiReferenceReader(Func<ISolidWorksSession> connect, TextWriter log) : IReferenceReader
{
    // RPC_E_CALL_REJECTED and RPC_E_SERVERCALL_RETRYLATER: SolidWorks is busy (loading, rebuilding).
    private static readonly int[] BusyHResults = [unchecked((int)0x80010001), unchecked((int)0x8001010A)];
    // The SolidWorks process is gone (closed, crashed, disconnected): the session can't be reused.
    private static readonly int[] DisconnectedHResults =
        [unchecked((int)0x80010108), unchecked((int)0x80010007), unchecked((int)0x80010012),
         unchecked((int)0x800706BA), unchecked((int)0x800706BE), unchecked((int)0x800706BF)];
    private const int BusyRetries = 5;
    private static readonly TimeSpan BusyDelay = TimeSpan.FromMilliseconds(500);

    private ISolidWorksSession? _session;

    public string Name => "solidworks";

    public IReadOnlyList<RefEntry> GetReferences(string path)
    {
        if (!File.Exists(path))
            throw new ReferenceReaderException(ErrorCodes.NotFound, $"file not found: {path}");

        var raw = Call(path, s => s.GetDocumentDependencies(path));
        return ParseDependencies(raw, File.Exists);
    }

    public void ReplaceReference(string document, string oldReference, string newReference)
    {
        if (!File.Exists(document))
            throw new ReferenceReaderException(ErrorCodes.NotFound, $"file not found: {document}");
        if (!File.Exists(newReference))
            throw new ReferenceReaderException(ErrorCodes.NotFound, $"file not found: {newReference}");
        var ok = Call(document, s => s.ReplaceReferencedDocument(document, oldReference, newReference));
        if (!ok)
            throw new ReferenceReaderException(ErrorCodes.OpenFailed,
                $"SolidWorks could not change {document} to use {newReference} (is it open in SolidWorks, or read-only?)");
    }

    public void Dispose()
    {
        _session?.Dispose();
        _session = null;
    }

    /// <summary>
    /// Turns SolidWorks' flat array of (name, full path) pairs into references. A pair with no
    /// path falls back to its name. Names containing '^' are virtual components.
    /// </summary>
    public static IReadOnlyList<RefEntry> ParseDependencies(object? raw, Func<string, bool> fileExists)
    {
        var items = raw switch
        {
            string[] s => s,
            object[] o => o.Select(x => x as string ?? "").ToArray(),
            _ => [],
        };

        var refs = new List<RefEntry>(items.Length / 2);
        for (var i = 0; i + 1 < items.Length; i += 2)
        {
            var path = string.IsNullOrWhiteSpace(items[i + 1]) ? items[i] : items[i + 1];
            if (string.IsNullOrWhiteSpace(path))
                continue;
            var isVirtual = PathRules.FileName(path).Contains('^');
            refs.Add(new RefEntry(path, !isVirtual && fileExists(path), isVirtual));
        }
        return refs;
    }

    private T Call<T>(string path, Func<ISolidWorksSession, T> call)
    {
        for (var attempt = 0; ; attempt++)
        {
            try
            {
                _session ??= connect();
                return call(_session);
            }
            catch (ReferenceReaderException)
            {
                throw;
            }
            catch (System.Runtime.InteropServices.COMException e) when (BusyHResults.Contains(e.HResult) && attempt < BusyRetries)
            {
                Thread.Sleep(BusyDelay);
            }
            catch (System.Runtime.InteropServices.COMException e) when (DisconnectedHResults.Contains(e.HResult))
            {
                // SolidWorks closed or crashed since the last request: reconnect on the next one.
                log.WriteLine($"swrefs: SolidWorks went away: 0x{e.HResult:X8} {e.Message}");
                _session?.Dispose();
                _session = null;
                throw new ReferenceReaderException(ErrorCodes.OpenFailed,
                    $"SolidWorks closed while reading {path} (0x{e.HResult:X8}). Try again.");
            }
            catch (System.Runtime.InteropServices.COMException e)
            {
                // SolidWorks is alive but refused this call. Keep the session: tearing it down would
                // close a hidden SolidWorks that took a minute to start, for nothing.
                log.WriteLine($"swrefs: SolidWorks call failed: 0x{e.HResult:X8} {e.Message}");
                throw new ReferenceReaderException(ErrorCodes.OpenFailed,
                    $"SolidWorks could not read {path} (0x{e.HResult:X8}: {e.Message.Trim()}).");
            }
        }
    }
}
