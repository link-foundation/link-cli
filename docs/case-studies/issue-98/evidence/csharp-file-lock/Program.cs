// Cross-process check for the advisory locking used by
// Foundation.Data.Doublets.Cli.LinksFileLock (issue #98, ask #4).
//
//   dotnet run -- hold <path> <shared|exclusive> <milliseconds>
//   dotnet run -- try  <path> <shared|exclusive>
//
// `try` prints "acquired" or "blocked" and exits 0/1 accordingly, so a
// shell script can assert the sharing matrix across real processes.
var path = args[1];
var mode = args[2] == "exclusive" ? FileAccess.ReadWrite : FileAccess.Read;
var share = args[2] == "exclusive" ? FileShare.None : FileShare.Read;

if (args[0] == "hold")
{
    using var stream = new FileStream(path, FileMode.OpenOrCreate, mode, share);
    Console.WriteLine("held");
    Console.Out.Flush();
    Thread.Sleep(int.Parse(args[3]));
    return 0;
}

try
{
    using var stream = new FileStream(path, FileMode.OpenOrCreate, mode, share);
    Console.WriteLine("acquired");
    return 0;
}
catch (Exception ex) when (ex is IOException or UnauthorizedAccessException)
{
    Console.WriteLine("blocked");
    return 1;
}
