using System.Text.Json;
using System.Text.RegularExpressions;
using SwRefs.Protocol;
using SwRefs.Readers;
using static SwRefs.Tests.TestSupport;

namespace SwRefs.Tests;

public class ProtocolTests
{
    private readonly RequestHandler _handler = SampleHandler();

    [Fact]
    public void Ping_ReturnsOkWithSemverVersion()
    {
        var r = _handler.Handle(new { id = "p1", op = "ping" });

        Assert.Equal("p1", r.GetProperty("id").GetString());
        Assert.True(r.GetProperty("ok").GetBoolean());
        Assert.Matches(new Regex(@"^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$"), r.GetProperty("version").GetString());
        Assert.Equal(3, r.EnumerateObject().Count());
    }

    [Fact]
    public void Refs_ReturnsDirectRefsFromFake()
    {
        var r = _handler.Handle(new { id = "r1", op = "refs", path = @"C:\Vault\Arm\Arm.SLDASM" });

        Assert.Equal("r1", r.GetProperty("id").GetString());
        Assert.True(r.GetProperty("ok").GetBoolean());
        var only = Assert.Single(r.GetProperty("refs").EnumerateArray().ToList());
        Assert.Equal(@"C:\Vault\Arm\Shoulder.SLDPRT", only.GetProperty("path").GetString());
        Assert.Equal("Shoulder.SLDPRT", only.GetProperty("fileName").GetString());
        Assert.True(only.GetProperty("exists").GetBoolean());
        Assert.False(only.GetProperty("isVirtual").GetBoolean());
    }

    [Fact]
    public void Refs_FlagsVirtualAndMissing_AndDeduplicatesCaseInsensitively()
    {
        var r = _handler.Handle(new { id = "r2", op = "refs", path = @"c:\vault\robot.sldasm" });

        var refs = r.GetProperty("refs").EnumerateArray().ToList();
        Assert.Equal(
            [@"C:\Vault\Drivetrain\Drivetrain.SLDASM", @"C:\Vault\Arm\Arm.SLDASM", @"C:\Users\alex\Desktop\Bracket.SLDPRT", "Spacer^Robot.sldprt"],
            refs.Select(x => x.GetProperty("path").GetString()));

        var bracket = refs[2];
        Assert.False(bracket.GetProperty("exists").GetBoolean());
        Assert.Equal("Bracket.SLDPRT", bracket.GetProperty("fileName").GetString());

        var spacer = refs[3];
        Assert.True(spacer.GetProperty("isVirtual").GetBoolean());
        Assert.Equal("Spacer^Robot.sldprt", spacer.GetProperty("fileName").GetString());
    }

    [Fact]
    public void Refs_PartWithNoRefs_ReturnsEmptyArray()
    {
        var r = _handler.Handle(new { id = "r3", op = "refs", path = @"C:\Vault\Arm\Shoulder.SLDPRT" });

        Assert.True(r.GetProperty("ok").GetBoolean());
        Assert.Empty(r.GetProperty("refs").EnumerateArray());
    }

    [Fact]
    public void Refs_Drawing_IsSupported()
    {
        var r = _handler.Handle(new { id = "r4", op = "refs", path = @"C:\Vault\Arm\Shoulder.SLDDRW" });

        Assert.True(r.GetProperty("ok").GetBoolean());
        Assert.Single(r.GetProperty("refs").EnumerateArray());
    }

    [Fact]
    public void Refs_ExistsComesFromDiskWhenFixtureOmitsIt()
    {
        var dir = Directory.CreateTempSubdirectory("swrefs-test-");
        try
        {
            var present = Path.Combine(dir.FullName, "Present.SLDPRT");
            File.WriteAllText(present, "");
            var absent = Path.Combine(dir.FullName, "Absent.SLDPRT");
            var top = Path.Combine(dir.FullName, "Top.SLDASM");
            var fixture = JsonSerializer.Serialize(new Dictionary<string, object> { [top] = new[] { present, absent } });
            var handler = new RequestHandler(FakeReferenceReader.FromJson(fixture), TextWriter.Null);

            var refs = handler.Handle(new { id = "d", op = "refs", path = top }).GetProperty("refs").EnumerateArray().ToList();

            Assert.True(refs[0].GetProperty("exists").GetBoolean());
            Assert.False(refs[1].GetProperty("exists").GetBoolean());
        }
        finally
        {
            dir.Delete(recursive: true);
        }
    }

    [Fact]
    public void Refs_PathNotInFixture_IsNotFound()
    {
        var r = _handler.Handle(new { id = "nf", op = "refs", path = @"C:\Vault\Nope.SLDASM" });
        AssertError(r, "nf", ErrorCodes.NotFound);
    }

    [Theory]
    [InlineData(@"C:\Vault\Corrupt.SLDASM", ErrorCodes.OpenFailed)]
    [InlineData(@"C:\Vault\Locked.SLDPRT", ErrorCodes.NoLicense)]
    public void Refs_ReaderErrors_PassThroughTheirCode(string path, string code)
    {
        var r = _handler.Handle(new { id = "e", op = "refs", path });
        AssertError(r, "e", code);
    }

    [Fact]
    public void Refs_UnexpectedReaderException_IsOpenFailed_AndHandlerKeepsWorking()
    {
        var handler = new RequestHandler(new ThrowingReader(), TextWriter.Null);

        AssertError(handler.Handle(new { id = "x", op = "refs", path = @"C:\a.SLDPRT" }), "x", ErrorCodes.OpenFailed);
        Assert.True(handler.Handle(new { id = "y", op = "ping" }).GetProperty("ok").GetBoolean());
    }

    [Theory]
    [InlineData("{not json")]
    [InlineData("{\"id\":\"a\",\"op\":")]
    [InlineData("\u0001")]
    public void BadJson_IsBadRequestWithNullId(string line)
    {
        AssertError(Parse(_handler.Handle(line)), null, ErrorCodes.BadRequest);
    }

    [Theory]
    [InlineData("[1,2,3]")]
    [InlineData("\"ping\"")]
    [InlineData("42")]
    [InlineData("null")]
    public void NonObjectJson_IsBadRequest(string line)
    {
        AssertError(Parse(_handler.Handle(line)), null, ErrorCodes.BadRequest);
    }

    [Theory]
    [InlineData("{\"op\":\"ping\"}")]
    [InlineData("{\"id\":7,\"op\":\"ping\"}")]
    [InlineData("{\"id\":null,\"op\":\"ping\"}")]
    public void MissingOrNonStringId_IsBadRequestWithNullId(string line)
    {
        AssertError(Parse(_handler.Handle(line)), null, ErrorCodes.BadRequest);
    }

    [Theory]
    [InlineData("{\"id\":\"m\"}")]
    [InlineData("{\"id\":\"m\",\"op\":5}")]
    public void MissingOp_IsBadRequestWithId(string line)
    {
        AssertError(Parse(_handler.Handle(line)), "m", ErrorCodes.BadRequest);
    }

    [Fact]
    public void UnknownOp_IsBadRequestWithId()
    {
        var r = _handler.Handle(new { id = "u", op = "explode" });
        AssertError(r, "u", ErrorCodes.BadRequest);
        Assert.Contains("explode", r.GetProperty("error").GetString());
    }

    [Theory]
    [InlineData("{\"id\":\"mp\",\"op\":\"refs\"}")]
    [InlineData("{\"id\":\"mp\",\"op\":\"refs\",\"path\":\"\"}")]
    [InlineData("{\"id\":\"mp\",\"op\":\"refs\",\"path\":\"   \"}")]
    [InlineData("{\"id\":\"mp\",\"op\":\"refs\",\"path\":null}")]
    [InlineData("{\"id\":\"mp\",\"op\":\"refs\",\"path\":[\"C:\\\\a.sldprt\"]}")]
    public void MissingPath_IsBadRequest(string line)
    {
        AssertError(Parse(_handler.Handle(line)), "mp", ErrorCodes.BadRequest);
    }

    [Theory]
    [InlineData(@"Robot.SLDASM")]
    [InlineData(@"Vault\Robot.SLDASM")]
    [InlineData(@"..\Robot.SLDASM")]
    [InlineData("C:\\Vault\\Ro\0bot.SLDASM")]
    public void RelativeOrInvalidPath_IsBadRequest(string path)
    {
        AssertError(_handler.Handle(new { id = "rel", op = "refs", path }), "rel", ErrorCodes.BadRequest);
    }

    [Theory]
    [InlineData(@"C:\Vault\Robot.step")]
    [InlineData(@"C:\Vault\Robot.SLDASM.bak")]
    [InlineData(@"C:\Vault\~$Robot.txt")]
    [InlineData(@"C:\Vault\.sldprt")]
    public void UnsupportedExtension_IsUnsupported(string path)
    {
        AssertError(_handler.Handle(new { id = "ext", op = "refs", path }), "ext", ErrorCodes.Unsupported);
    }

    [Theory]
    [InlineData("")]
    [InlineData("   ")]
    [InlineData("\t")]
    public void BlankLine_GetsNoResponse(string line)
    {
        Assert.Null(_handler.Handle(line));
    }

    [Fact]
    public void ResponsesStaySingleLine_ForAwkwardIds()
    {
        var id = "line1\nline2 \"quoted\" é ✓";
        var r = Parse(_handler.Handle(JsonSerializer.Serialize(new { id, op = "ping" })));
        Assert.Equal(id, r.GetProperty("id").GetString());
    }

    [Fact]
    public void Server_AnswersEachNonBlankLineInOrder_AndSurvivesBadInput()
    {
        var input = new StringReader(string.Join("\n",
            "{\"id\":\"1\",\"op\":\"ping\"}",
            "garbage",
            "",
            "{\"id\":\"3\",\"op\":\"refs\",\"path\":\"C:\\\\Vault\\\\Arm\\\\Arm.SLDASM\"}",
            "{\"id\":\"4\",\"op\":\"nope\"}",
            "{\"id\":\"5\",\"op\":\"ping\"}"));
        var output = new StringWriter();

        Server.Run(input, output, _handler);

        var lines = output.ToString().Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(Parse).ToList();
        Assert.Equal(5, lines.Count);
        Assert.Equal(["1", null, "3", "4", "5"], lines.Select(l => l.GetProperty("id").GetString()));
        Assert.Equal([true, false, true, false, true], lines.Select(l => l.GetProperty("ok").GetBoolean()));
    }

    private sealed class ThrowingReader : IReferenceReader
    {
        public string Name => "throwing";
        public IReadOnlyList<RefEntry> GetReferences(string path) => throw new InvalidOperationException("boom");
        public void Dispose() { }
    }
}
