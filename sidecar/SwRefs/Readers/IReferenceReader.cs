namespace SwRefs.Readers;

/// <summary>One reference stored in a SolidWorks file.</summary>
/// <param name="Path">Absolute path as stored in the referencing file.</param>
/// <param name="Exists">Whether a file exists at <paramref name="Path"/> on this machine.</param>
/// <param name="IsVirtual">True for virtual components, which live inside the assembly file.</param>
public sealed record RefEntry(string Path, bool Exists, bool IsVirtual);

/// <summary>Reads the direct (one-level) external references of a SolidWorks file.</summary>
public interface IReferenceReader : IDisposable
{
    /// <summary>Short name for logs, e.g. "solidworks" or "fake".</summary>
    string Name { get; }

    /// <summary>
    /// Returns the direct references of the file at <paramref name="path"/>.
    /// Throws <see cref="ReferenceReaderException"/> for expected failures.
    /// </summary>
    IReadOnlyList<RefEntry> GetReferences(string path);

    /// <summary>
    /// Makes the closed file at <paramref name="document"/> reference <paramref name="newReference"/>
    /// wherever it referenced <paramref name="oldReference"/> (the path as stored). Used when a
    /// referenced file is renamed or moved. Throws <see cref="ReferenceReaderException"/> on failure.
    /// </summary>
    void ReplaceReference(string document, string oldReference, string newReference) =>
        throw new ReferenceReaderException(Protocol.ErrorCodes.Unsupported, $"{Name} can't rewrite references");
}

/// <summary>An expected failure carrying one of the protocol's <see cref="Protocol.ErrorCodes"/>.</summary>
public sealed class ReferenceReaderException(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}
