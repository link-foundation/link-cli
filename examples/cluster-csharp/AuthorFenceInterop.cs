using System.Globalization;
using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

internal static class AuthorFenceInterop
{
    public static int Etcd(string[] args)
    {
        if (args.Length != 5) throw new ArgumentException("Use --claim-author endpoints prefix/ author predecessor token");
        using var store = new EtcdJournalStore(args[0].Split(',').Select(value => new Uri(value)), args[1]);
        var claim = new AuthorFence(store, "fence", args[2], new(RequireDurableFlush: true)).Claim(ulong.Parse(args[3], CultureInfo.InvariantCulture), args[4]);
        Console.WriteLine(JsonSerializer.Serialize(new { author = claim.Author, token = claim.Token, epoch = claim.Epoch.ToString(CultureInfo.InvariantCulture) }));
        return 0;
    }
}
