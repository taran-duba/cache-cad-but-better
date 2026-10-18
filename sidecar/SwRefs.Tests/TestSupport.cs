using System.Text.Json;
using SwRefs.Protocol;
using SwRefs.Readers;

namespace SwRefs.Tests;

internal static class TestSupport
{
    public static string SampleFixturePath => Path.Combine(AppContext.BaseDirectory, "fixtures", "sample-refs.json");

    public static RequestHandler SampleHandler() =>
        new(FakeReferenceReader.FromFile(SampleFixturePath), TextWriter.Null);

    public static JsonElement Parse(string? line)
    {
        Assert.NotNull(line);
        Assert.DoesNotContain('\n', line);
        Assert.DoesNotContain('\r', line);
        using var doc = JsonDocument.Parse(line);
        return doc.RootElement.Clone();
    }

    public static JsonElement Handle(this RequestHandler handler, object request) =>
        Parse(handler.Handle(JsonSerializer.Serialize(request)));

    public static void AssertError(JsonElement response, string? id, string code)
    {
        Assert.False(response.GetProperty("ok").GetBoolean(), response.ToString());
        Assert.Equal(code, response.GetProperty("code").GetString());
        Assert.False(string.IsNullOrEmpty(response.GetProperty("error").GetString()));
        if (id is null)
            Assert.Equal(JsonValueKind.Null, response.GetProperty("id").ValueKind);
        else
            Assert.Equal(id, response.GetProperty("id").GetString());
    }
}
