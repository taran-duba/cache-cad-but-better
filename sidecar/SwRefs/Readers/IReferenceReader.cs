namespace SwRefs.Readers;

/// <summary>One reference stored in a SolidWorks file.</summary>
/// <param name="Path">Absolute path as stored in the referencing file.</param>
/// <param name="Exists">Whether a file exists at <paramref name="Path"/> on this machine.</param>
/// <param name="IsVirtual">True for virtual components, which live inside the assembly file.</param>
public sealed record RefEntry(string Path, bool Exists, bool IsVirtual);

/// <summary>Reads the direct (one-level) external references of a SolidWorks file.</summary>
public interface IReferenceReader : IDisposable
{
    /// <summary>Short name for logs, e.g. "swdm" or "fake".</summary>
    string Name { get; }

    /// <summary>
    /// Returns the direct references of the file at <paramref name="path"/>.
    /// Throws <see cref="ReferenceReaderException"/> for expected failures.
    /// </summary>
    IReadOnlyList<RefEntry> GetReferences(string path);
}

/// <summary>An expected failure carrying one of the protocol's <see cref="Protocol.ErrorCodes"/>.</summary>
public sealed class ReferenceReaderException(string code, string message) : Exception(message)
{
    public string Code { get; } = code;
}
