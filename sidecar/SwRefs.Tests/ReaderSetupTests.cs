using SwRefs.Protocol;
using SwRefs.Readers;
using static SwRefs.Tests.TestSupport;

namespace SwRefs.Tests;

public class ReaderSetupTests
{
    [Fact]
    public void FakeEnvVar_SelectsFakeReader()
    {
        using var reader = Program.CreateReader(Env((Program.FakeEnvVar, SampleFixturePath)), TextWriter.Null);
        Assert.IsType<FakeReferenceReader>(reader);
    }

    [Fact]
    public void MissingFixtureFile_KeepsRunning_AndRefsReportOpenFailed()
    {
        var log = new StringWriter();
        using var reader = Program.CreateReader(Env((Program.FakeEnvVar, "/definitely/not/here.json")), log);
        var handler = new RequestHandler(reader, TextWriter.Null);

        Assert.True(handler.Handle(new { id = "p", op = "ping" }).GetProperty("ok").GetBoolean());
        AssertError(handler.Handle(new { id = "r", op = "refs", path = @"C:\a.SLDASM" }), "r", ErrorCodes.OpenFailed);
        Assert.Contains("could not be loaded", log.ToString());
    }

#if !SWDM
    [Fact]
    public void WithoutInterop_RefsReportUnsupported()
    {
        using var reader = Program.CreateReader(Env((Program.KeyEnvVar, "not-a-real-key")), TextWriter.Null);
        var handler = new RequestHandler(reader, TextWriter.Null);

        AssertError(handler.Handle(new { id = "r", op = "refs", path = @"C:\a.SLDASM" }), "r", ErrorCodes.Unsupported);
    }
#endif

    [Theory]
    [InlineData("[]")]
    [InlineData("{ \"C:\\\\a.SLDASM\": 5 }")]
    [InlineData("{ \"C:\\\\a.SLDASM\": { \"error\": \"BOGUS\" } }")]
    [InlineData("{ \"C:\\\\a.SLDASM\": { \"message\": \"no code\" } }")]
    [InlineData("{ \"C:\\\\a.SLDASM\": [ { \"exists\": true } ] }")]
    [InlineData("{ \"C:\\\\a.SLDASM\": [ 1 ] }")]
    [InlineData("{ broken")]
    public void MalformedFixture_IsFormatException(string json)
    {
        Assert.Throws<FormatException>(() => FakeReferenceReader.FromJson(json));
    }

    private static Func<string, string?> Env(params (string Key, string Value)[] vars) =>
        name => vars.FirstOrDefault(v => v.Key == name).Value;
}
