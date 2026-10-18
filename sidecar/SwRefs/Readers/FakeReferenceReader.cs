using System.Text.Json;
using SwRefs.Protocol;

namespace SwRefs.Readers;

/// <summary>
/// Answers from a JSON fixture instead of reading SolidWorks files. Selected when
/// CADSYNC_SWREFS_FAKE points at a fixture file. Fixture shape:
/// <code>
/// {
///   "C:\\Vault\\Robot.SLDASM": [
///     { "path": "C:\\Vault\\Arm.SLDASM", "exists": true },
///     "C:\\Vault\\Base.SLDPRT",
///     { "path": "Bracket^Robot.sldprt", "isVirtual": true }
///   ],
///   "C:\\Vault\\Corrupt.SLDASM": { "error": "OPEN_FAILED", "message": "file is damaged" }
/// }
/// </code>
/// Keys match case-insensitively. A ref given as a bare string, or without "exists",
/// gets <c>exists</c> from the real file system. Paths missing from the fixture return NOT_FOUND.
/// </summary>
public sealed class FakeReferenceReader : IReferenceReader
{
    private readonly Dictionary<string, Func<IReadOnlyList<RefEntry>>> _entries;

    private FakeReferenceReader(Dictionary<string, Func<IReadOnlyList<RefEntry>>> entries) => _entries = entries;

    public string Name => "fake";

    public static FakeReferenceReader FromFile(string fixturePath) => FromJson(File.ReadAllText(fixturePath));

    /// <summary>Parses a fixture. Throws <see cref="FormatException"/> when it is malformed.</summary>
    public static FakeReferenceReader FromJson(string json)
    {
        JsonDocument doc;
        try
        {
            doc = JsonDocument.Parse(json, new JsonDocumentOptions { CommentHandling = JsonCommentHandling.Skip, AllowTrailingCommas = true });
        }
        catch (JsonException e)
        {
            throw new FormatException($"fixture is not valid JSON: {e.Message}", e);
        }

        using (doc)
        {
            if (doc.RootElement.ValueKind != JsonValueKind.Object)
                throw new FormatException("fixture root must be an object mapping file paths to refs");

            var entries = new Dictionary<string, Func<IReadOnlyList<RefEntry>>>(StringComparer.OrdinalIgnoreCase);
            foreach (var prop in doc.RootElement.EnumerateObject())
                entries[prop.Name] = ParseEntry(prop.Name, prop.Value);
            return new FakeReferenceReader(entries);
        }
    }

    public IReadOnlyList<RefEntry> GetReferences(string path)
    {
        if (!_entries.TryGetValue(path, out var entry))
            throw new ReferenceReaderException(ErrorCodes.NotFound, $"file not found: {path} (not in fake fixture)");
        return entry();
    }

    public void Dispose() { }

    private static Func<IReadOnlyList<RefEntry>> ParseEntry(string key, JsonElement value)
    {
        if (value.ValueKind == JsonValueKind.Object)
        {
            var code = GetString(value, "error") ?? throw new FormatException($"fixture entry '{key}': object entries need an \"error\" code");
            if (!ErrorCodes.All.Contains(code))
                throw new FormatException($"fixture entry '{key}': unknown error code '{code}'");
            var message = GetString(value, "message") ?? code;
            return () => throw new ReferenceReaderException(code, message);
        }

        if (value.ValueKind != JsonValueKind.Array)
            throw new FormatException($"fixture entry '{key}': expected an array of refs or an error object");

        var specs = new List<(string Path, bool? Exists, bool IsVirtual)>();
        foreach (var item in value.EnumerateArray())
        {
            switch (item.ValueKind)
            {
                case JsonValueKind.String:
                    specs.Add((item.GetString()!, null, false));
                    break;
                case JsonValueKind.Object:
                    var path = GetString(item, "path") ?? throw new FormatException($"fixture entry '{key}': ref is missing \"path\"");
                    specs.Add((path, GetBool(item, "exists"), GetBool(item, "isVirtual") ?? false));
                    break;
                default:
                    throw new FormatException($"fixture entry '{key}': refs must be strings or objects");
            }
        }

        // "exists" is resolved per request so fixtures can point at temp files a test creates later.
        return () => specs.Select(s => new RefEntry(s.Path, s.Exists ?? File.Exists(s.Path), s.IsVirtual)).ToList();
    }

    private static string? GetString(JsonElement obj, string name) =>
        obj.TryGetProperty(name, out var v) && v.ValueKind == JsonValueKind.String ? v.GetString() : null;

    private static bool? GetBool(JsonElement obj, string name) =>
        obj.TryGetProperty(name, out var v) && v.ValueKind is JsonValueKind.True or JsonValueKind.False ? v.GetBoolean() : null;
}
