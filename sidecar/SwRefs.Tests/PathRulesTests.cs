using SwRefs.Protocol;

namespace SwRefs.Tests;

public class PathRulesTests
{
    [Theory]
    [InlineData(@"C:\Vault\Arm\Arm.SLDASM", "Arm.SLDASM")]
    [InlineData("C:/Vault/Arm.SLDASM", "Arm.SLDASM")]
    [InlineData(@"\\server\share\Base.SLDPRT", "Base.SLDPRT")]
    [InlineData("Spacer^Robot.sldprt", "Spacer^Robot.sldprt")]
    [InlineData("/Users/me/Vault/Base.SLDPRT", "Base.SLDPRT")]
    public void FileName_HandlesBothSeparators(string path, string expected)
    {
        Assert.Equal(expected, PathRules.FileName(path));
    }

    [Theory]
    [InlineData(@"C:\Vault\a.sldprt", true)]
    [InlineData("d:/Vault/a.sldprt", true)]
    [InlineData(@"\\server\share\a.sldprt", true)]
    [InlineData(@"C:a.sldprt", false)]
    [InlineData(@"Vault\a.sldprt", false)]
    [InlineData(@"\\", false)]
    [InlineData("a.sldprt", false)]
    public void IsAbsolute_WindowsForms(string path, bool expected)
    {
        Assert.Equal(expected, PathRules.IsAbsolute(path));
    }

    [Fact]
    public void IsAbsolute_UnixRooted_OnlyOffWindows()
    {
        Assert.Equal(!OperatingSystem.IsWindows(), PathRules.IsAbsolute("/Users/me/a.sldprt"));
    }
}
