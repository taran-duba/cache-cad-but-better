using System.Text.Encodings.Web;
using System.Text.Json;
using SwRefs.Readers;

namespace SwRefs.Protocol;

/// <summary>
/// Turns one request line into one response line. Never throws: every failure becomes an
/// error response, so one bad request cannot take the process down.
/// </summary>
public sealed class RequestHandler(IReferenceReader reader, TextWriter log)
{
    private static readonly JsonWriterOptions WriterOptions = new()
    {
        // Keep non-ASCII paths readable; quotes, control characters and newlines are still escaped.
        Encoder = JavaScriptEncoder.UnsafeRelaxedJsonEscaping,
    };

    /// <summary>Returns the response line, or null for a blank line (which gets no response).</summary>
    public string? Handle(string line)
    {
        if (string.IsNullOrWhiteSpace(line))
            return null;

        string? id = null;
        try
        {
            using var doc = JsonDocument.Parse(line);
            var root = doc.RootElement;
            if (root.ValueKind != JsonValueKind.Object)
                return Error(null, ErrorCodes.BadRequest, "request must be a JSON object");

            if (!root.TryGetProperty("id", out var idEl) || idEl.ValueKind != JsonValueKind.String)
                return Error(null, ErrorCodes.BadRequest, "request needs a string \"id\"");
            id = idEl.GetString();

            if (!root.TryGetProperty("op", out var opEl) || opEl.ValueKind != JsonValueKind.String)
                return Error(id, ErrorCodes.BadRequest, "request needs a string \"op\"");

            return opEl.GetString() switch
            {
                "ping" => Ping(id),
                "refs" => Refs(id, root),
                var op => Error(id, ErrorCodes.BadRequest, $"unknown op \"{op}\""),
            };
        }
        catch (JsonException e)
        {
            return Error(id, ErrorCodes.BadRequest, $"invalid JSON: {e.Message}");
        }
        catch (Exception e)
        {
            log.WriteLine($"swrefs: unexpected error handling request {id ?? "(no id)"}: {e}");
            return Error(id, ErrorCodes.OpenFailed, $"internal error: {e.Message}");
        }
    }

    private static string Ping(string? id) => Write(w =>
    {
        w.WriteString("id", id);
        w.WriteBoolean("ok", true);
        w.WriteString("version", ProtocolInfo.Version);
    });

    private string Refs(string? id, JsonElement root)
    {
        if (!root.TryGetProperty("path", out var pathEl) || pathEl.ValueKind != JsonValueKind.String || string.IsNullOrWhiteSpace(pathEl.GetString()))
            return Error(id, ErrorCodes.BadRequest, "\"refs\" needs a non-empty string \"path\"");

        var path = pathEl.GetString()!;
        if (path.Contains('\0') || !PathRules.IsAbsolute(path))
            return Error(id, ErrorCodes.BadRequest, $"path must be absolute: {path}");
        if (!PathRules.HasSupportedExtension(path))
            return Error(id, ErrorCodes.Unsupported, $"not a SolidWorks part, assembly or drawing: {path}");

        IReadOnlyList<RefEntry> refs;
        try
        {
            refs = reader.GetReferences(path);
        }
        catch (ReferenceReaderException e)
        {
            log.WriteLine($"swrefs: refs {path}: {e.Code} {e.Message}");
            return Error(id, e.Code, e.Message);
        }
        catch (Exception e)
        {
            log.WriteLine($"swrefs: refs {path} failed: {e}");
            return Error(id, ErrorCodes.OpenFailed, $"could not read references of {path}: {e.Message}");
        }

        // Windows paths are case-insensitive, so "Arm.SLDPRT" and "arm.sldprt" are the same reference.
        var seen = new HashSet<string>(StringComparer.OrdinalIgnoreCase);
        return Write(w =>
        {
            w.WriteString("id", id);
            w.WriteBoolean("ok", true);
            w.WriteStartArray("refs");
            foreach (var r in refs)
            {
                if (string.IsNullOrEmpty(r.Path) || !seen.Add(r.Path))
                    continue;
                w.WriteStartObject();
                w.WriteString("path", r.Path);
                w.WriteString("fileName", PathRules.FileName(r.Path));
                w.WriteBoolean("exists", r.Exists);
                w.WriteBoolean("isVirtual", r.IsVirtual);
                w.WriteEndObject();
            }
            w.WriteEndArray();
        });
    }

    private static string Error(string? id, string code, string message) => Write(w =>
    {
        w.WriteString("id", id);
        w.WriteBoolean("ok", false);
        w.WriteString("error", message);
        w.WriteString("code", code);
    });

    private static string Write(Action<Utf8JsonWriter> body)
    {
        using var buffer = new MemoryStream();
        using (var w = new Utf8JsonWriter(buffer, WriterOptions))
        {
            w.WriteStartObject();
            body(w);
            w.WriteEndObject();
        }
        return System.Text.Encoding.UTF8.GetString(buffer.ToArray());
    }
}
