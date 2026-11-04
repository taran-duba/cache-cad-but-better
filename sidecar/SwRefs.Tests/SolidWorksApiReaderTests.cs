using System.Runtime.InteropServices;
using SwRefs.Protocol;
using SwRefs.Readers;
using static SwRefs.Tests.TestSupport;

namespace SwRefs.Tests;

public class SolidWorksApiReaderTests
{
    private sealed class FakeSession(Func<string, object?> answer) : ISolidWorksSession
    {
        public int Calls;
        public bool Disposed;
        public object? GetDocumentDependencies(string path) { Calls++; return answer(path); }
        public void Dispose() => Disposed = true;
    }

    [Fact]
    public void ParseDependencies_ReadsNamePathPairs_AndMarksVirtualComponents()
    {
        object?[] raw =
        [
            "Arm.SLDASM", @"C:\Vault\Arm\Arm.SLDASM",
            "Gone.SLDPRT", @"C:\Vault\Gone.SLDPRT",
            "Spacer^Robot.sldprt", "Spacer^Robot.sldprt",
            "NoPath.SLDPRT", "",
            "Dangling.SLDPRT",
        ];
        var refs = SolidWorksApiReferenceReader.ParseDependencies(
            raw.Select(x => x).ToArray(), p => p == @"C:\Vault\Arm\Arm.SLDASM");

        Assert.Equal(
            [
                new RefEntry(@"C:\Vault\Arm\Arm.SLDASM", true, false),
                new RefEntry(@"C:\Vault\Gone.SLDPRT", false, false),
                new RefEntry("Spacer^Robot.sldprt", false, true),
                new RefEntry("NoPath.SLDPRT", false, false),
            ],
            refs);
    }

    [Theory]
    [InlineData(null)]
    [InlineData("not an array")]
    public void ParseDependencies_NothingUsable_IsEmpty(object? raw) =>
        Assert.Empty(SolidWorksApiReferenceReader.ParseDependencies(raw, _ => true));

    [Fact]
    public void ParseDependencies_AcceptsStringArrays()
    {
        var refs = SolidWorksApiReferenceReader.ParseDependencies(new[] { "A.SLDPRT", @"C:\A.SLDPRT" }, _ => true);
        Assert.Single(refs);
    }

    [Fact]
    public void MissingFile_IsNotFound_WithoutContactingSolidWorks()
    {
        var connects = 0;
        using var reader = new SolidWorksApiReferenceReader(() => { connects++; return new FakeSession(_ => null); }, TextWriter.Null);

        var e = Assert.Throws<ReferenceReaderException>(() => reader.GetReferences(Path.Combine(Path.GetTempPath(), "nope", "A.SLDASM")));
        Assert.Equal(ErrorCodes.NotFound, e.Code);
        Assert.Equal(0, connects);
    }

    [Fact]
    public void ConnectsOnce_AndReusesTheSession()
    {
        using var file = TempFile("A.SLDASM");
        var session = new FakeSession(_ => new[] { "P.SLDPRT", @"C:\P.SLDPRT" });
        var connects = 0;
        using var reader = new SolidWorksApiReferenceReader(() => { connects++; return session; }, TextWriter.Null);

        Assert.Single(reader.GetReferences(file.Path));
        Assert.Single(reader.GetReferences(file.Path));
        Assert.Equal(1, connects);
        Assert.Equal(2, session.Calls);
    }

    [Fact]
    public void ConnectFailure_PassesItsCodeThrough()
    {
        using var file = TempFile("A.SLDASM");
        using var reader = new SolidWorksApiReferenceReader(
            () => throw new ReferenceReaderException(ErrorCodes.Unsupported, "SolidWorks is not installed"), TextWriter.Null);

        var e = Assert.Throws<ReferenceReaderException>(() => reader.GetReferences(file.Path));
        Assert.Equal(ErrorCodes.Unsupported, e.Code);
    }

    [Fact]
    public void BusySolidWorks_IsRetried()
    {
        using var file = TempFile("A.SLDASM");
        var attempts = 0;
        var session = new FakeSession(_ =>
            ++attempts < 3 ? throw new COMException("busy", unchecked((int)0x8001010A)) : new[] { "P.SLDPRT", @"C:\P.SLDPRT" });
        using var reader = new SolidWorksApiReferenceReader(() => session, TextWriter.Null);

        Assert.Single(reader.GetReferences(file.Path));
        Assert.Equal(3, attempts);
    }

    [Fact]
    public void DisconnectedSolidWorks_ReportsOpenFailed_ThenReconnects()
    {
        using var file = TempFile("A.SLDASM");
        var connects = 0;
        var sessions = new List<FakeSession>();
        using var reader = new SolidWorksApiReferenceReader(() =>
        {
            var first = ++connects == 1;
            var s = new FakeSession(_ => first
                ? throw new COMException("gone", unchecked((int)0x80010108))
                : new[] { "P.SLDPRT", @"C:\P.SLDPRT" });
            sessions.Add(s);
            return s;
        }, TextWriter.Null);

        var e = Assert.Throws<ReferenceReaderException>(() => reader.GetReferences(file.Path));
        Assert.Equal(ErrorCodes.OpenFailed, e.Code);
        Assert.True(sessions[0].Disposed);

        Assert.Single(reader.GetReferences(file.Path));
        Assert.Equal(2, connects);
    }

    [Fact]
    public void RefusedCall_KeepsTheSession_AndReportsTheHResult()
    {
        using var file = TempFile("A.SLDASM");
        var connects = 0;
        var session = new FakeSession(_ => throw new COMException("Element not found", unchecked((int)0x8002802B)));
        using var reader = new SolidWorksApiReferenceReader(() => { connects++; return session; }, TextWriter.Null);

        var e = Assert.Throws<ReferenceReaderException>(() => reader.GetReferences(file.Path));
        Assert.Equal(ErrorCodes.OpenFailed, e.Code);
        Assert.Contains("0x8002802B", e.Message);
        Assert.False(session.Disposed);

        Assert.Throws<ReferenceReaderException>(() => reader.GetReferences(file.Path));
        Assert.Equal(1, connects);
    }

    [Fact]
    public void Dispose_ClosesTheSession()
    {
        using var file = TempFile("A.SLDASM");
        var session = new FakeSession(_ => null);
        var reader = new SolidWorksApiReferenceReader(() => session, TextWriter.Null);
        reader.GetReferences(file.Path);
        reader.Dispose();
        Assert.True(session.Disposed);
    }

    [Fact]
    public void NoKey_SelectsSolidWorksReader_OnWindows()
    {
        using var reader = Program.CreateReader(Env(), TextWriter.Null);
        Assert.Equal(OperatingSystem.IsWindows() ? "solidworks" : "unavailable", reader.Name);
    }

    private static Func<string, string?> Env(params (string Key, string Value)[] vars) =>
        name => vars.FirstOrDefault(v => v.Key == name).Value;

    private sealed class Temp(string path) : IDisposable
    {
        public string Path { get; } = path;
        public void Dispose() => File.Delete(Path);
    }

    private static Temp TempFile(string name)
    {
        var dir = Directory.CreateTempSubdirectory("swrefs-test-").FullName;
        var path = System.IO.Path.Combine(dir, name);
        File.WriteAllText(path, "x");
        return new Temp(path);
    }
}
