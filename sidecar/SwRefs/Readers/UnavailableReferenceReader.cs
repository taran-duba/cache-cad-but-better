namespace SwRefs.Readers;

/// <summary>
/// Used when no working reader could be set up (no interop compiled in, bad fixture).
/// The process stays up so "ping" still answers, and every "refs" request gets the same error.
/// </summary>
public sealed class UnavailableReferenceReader(string code, string message) : IReferenceReader
{
    public string Name => "unavailable";

    public IReadOnlyList<RefEntry> GetReferences(string path) =>
        throw new ReferenceReaderException(code, message);

    public void Dispose() { }
}
