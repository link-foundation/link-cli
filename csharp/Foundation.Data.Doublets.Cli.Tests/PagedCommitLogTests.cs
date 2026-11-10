using System.Text.Json;
using Foundation.Data.Doublets.Cli.Engine;

namespace Foundation.Data.Doublets.Cli.Tests;

public class PagedCommitLogTests
{
    [Fact]
    public void IndependentWireFilesReopenAndAcceptFollowingRecords()
    {
        using var vectors = JsonDocument.Parse(File.ReadAllText(Path.Combine(AppContext.BaseDirectory, "Protocol/native-pages-v1.json")));
        foreach (var name in new[] { "empty", "oneGroup", "twoGroups" })
        {
            var directory = Directory.CreateTempSubdirectory("native-pages-").FullName;
            var path = Path.Combine(directory, "journal");
            try
            {
                var fixture = vectors.RootElement.GetProperty(name);
                File.WriteAllBytes(path, fixture.GetProperty("pages").EnumerateArray().SelectMany(p => Convert.FromHexString(p.GetString()!)).ToArray());
                for (int slot = 0; slot < 2; slot++) File.WriteAllBytes(path + ".meta" + slot, Convert.FromHexString(fixture.GetProperty("metadata")[slot].GetString()!));
                var records = fixture.GetProperty("records").EnumerateArray().Select(p => p.GetString()!).ToArray();
                using (var device = new FilePageDevice(path))
                {
                    var log = new PagedCommitLog(device); Assert.True(log.SupportsDurableFlush); Assert.Equal(records, log.Read());
                    Assert.ThrowsAny<IOException>(() => new FilePageDevice(path));
                    log.Append("\ufeff雪\0😀"); log.Append(""); log.Flush();
                }
                using var reopened = new FilePageDevice(path);
                Assert.Equal(records.Concat(new[] { "\ufeff雪\0😀", "" }), new PagedCommitLog(reopened).Read());
            }
            finally { Directory.Delete(directory, recursive: true); }
        }
    }
}
