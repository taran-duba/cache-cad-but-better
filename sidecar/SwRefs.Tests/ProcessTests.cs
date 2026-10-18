using System.Diagnostics;
using System.Text;
using SwRefs.Protocol;
using static SwRefs.Tests.TestSupport;

namespace SwRefs.Tests;

/// <summary>Runs the real executable to check the stdin/stdout contract end to end.</summary>
public class ProcessTests
{
    [Fact]
    public async Task Process_WritesOnlyResponsesToStdout_AndExitsOnEof()
    {
        var (stdout, stderr, exitCode) = await RunSidecar(
            [
                "{\"id\":\"1\",\"op\":\"ping\"}",
                "this is not json",
                "{\"id\":\"3\",\"op\":\"refs\",\"path\":\"C:\\\\Vault\\\\Robot.SLDASM\"}",
                "{\"id\":\"4\",\"op\":\"refs\",\"path\":\"C:\\\\Vault\\\\Missing.SLDASM\"}",
            ],
            (Program.FakeEnvVar, SampleFixturePath));

        Assert.Equal(0, exitCode);
        Assert.EndsWith("\n", stdout);
        var lines = stdout.Split('\n', StringSplitOptions.RemoveEmptyEntries).Select(Parse).ToList();
        Assert.Equal(4, lines.Count);
        Assert.True(lines[0].GetProperty("ok").GetBoolean());
        AssertError(lines[1], null, ErrorCodes.BadRequest);
        Assert.Equal(4, lines[2].GetProperty("refs").GetArrayLength());
        AssertError(lines[3], "4", ErrorCodes.NotFound);
        Assert.Contains("reader: fake", stderr);
    }

    [Fact]
    public async Task Process_NeverLogsTheLicenseKey()
    {
        const string key = "secret-test-key-1234567890";
        var (stdout, stderr, _) = await RunSidecar(
            ["{\"id\":\"1\",\"op\":\"refs\",\"path\":\"C:\\\\Vault\\\\Robot.SLDASM\"}"],
            (Program.KeyEnvVar, key));

        Assert.DoesNotContain(key, stdout);
        Assert.DoesNotContain(key, stderr);
    }

    private static async Task<(string Stdout, string Stderr, int ExitCode)> RunSidecar(
        string[] requests, params (string Key, string Value)[] env)
    {
        var dll = typeof(Program).Assembly.Location;
        var psi = new ProcessStartInfo(DotnetHost())
        {
            RedirectStandardInput = true,
            RedirectStandardOutput = true,
            RedirectStandardError = true,
            UseShellExecute = false,
            StandardOutputEncoding = Encoding.UTF8,
            StandardErrorEncoding = Encoding.UTF8,
        };
        psi.ArgumentList.Add(dll);
        psi.Environment.Remove(Program.FakeEnvVar);
        psi.Environment.Remove(Program.KeyEnvVar);
        foreach (var (k, v) in env)
            psi.Environment[k] = v;

        using var process = Process.Start(psi)!;
        var stdoutTask = process.StandardOutput.ReadToEndAsync();
        var stderrTask = process.StandardError.ReadToEndAsync();
        foreach (var request in requests)
            await process.StandardInput.WriteAsync(request + "\n");
        process.StandardInput.Close();

        using var timeout = new CancellationTokenSource(TimeSpan.FromSeconds(30));
        await process.WaitForExitAsync(timeout.Token);
        return (await stdoutTask, await stderrTask, process.ExitCode);
    }

    private static string DotnetHost()
    {
        var hostPath = Environment.GetEnvironmentVariable("DOTNET_HOST_PATH");
        if (!string.IsNullOrEmpty(hostPath) && File.Exists(hostPath))
            return hostPath;
        var processPath = Environment.ProcessPath;
        if (processPath is not null && Path.GetFileNameWithoutExtension(processPath) == "dotnet")
            return processPath;
        return "dotnet";
    }
}
