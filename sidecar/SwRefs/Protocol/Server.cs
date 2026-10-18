namespace SwRefs.Protocol;

/// <summary>Reads request lines until end of input, writing one response line per request.</summary>
public static class Server
{
    public static void Run(TextReader input, TextWriter output, RequestHandler handler)
    {
        while (input.ReadLine() is { } line)
        {
            var response = handler.Handle(line);
            if (response is null)
                continue;
            output.Write(response);
            output.Write('\n');
            output.Flush();
        }
    }
}
