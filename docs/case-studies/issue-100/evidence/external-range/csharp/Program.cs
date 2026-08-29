// The reference behaviour the Rust program next door is compared against:
// `LinksConstants<uint>` with external-reference support, from Platform.Data
// (a transitive dependency of Platform.Data.Doublets 0.18.1).
//
// The six service constants sit at the top of the internal range, exactly as
// in `platform-data`, but the external range starts *one past* the half, so
// `IsExternalReference(Continue)` is False.
using Platform.Data;

var c = new LinksConstants<uint>(enableExternalReferencesSupport: true);
Console.WriteLine($"null      = {c.Null}");
Console.WriteLine($"continue  = {c.Continue}");
Console.WriteLine($"break     = {c.Break}");
Console.WriteLine($"skip      = {c.Skip}");
Console.WriteLine($"any       = {c.Any}");
Console.WriteLine($"itself    = {c.Itself}");
Console.WriteLine($"error     = {c.Error}");
Console.WriteLine($"internal  = {c.InternalReferencesRange}");
Console.WriteLine($"external  = {c.ExternalReferencesRange}");
Console.WriteLine();
Console.WriteLine($"IsExternalReference(continue) = {c.IsExternalReference(c.Continue)}");
