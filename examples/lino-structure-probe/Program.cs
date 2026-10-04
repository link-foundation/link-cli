// Prints how the C# links-notation parser structures the inputs the Rust
// protocol tests rely on, so both ports can be checked for agreement:
// `dotnet run --project examples/lino-structure-probe -- "((a))"`. The Rust
// counterpart is `cargo run --example lino_structure_probe -- "((a))"`.
using Link.Foundation.Links.Notation;

static string Show(Link<string> link) =>
    link.Values == null
        ? $"Ref({link.Id ?? "<null>"})"
        : $"Link{{{(link.Id == null ? "None" : link.Id)},[{string.Join(",", link.Values.Select(Show))}]}}";

var inputs = args.Length > 0 ? args : new[] {
    "() ((1 1))", "(a b)", "a b", "a", "(a)", "((a))", "(((a)))", "(a:)", "(a: b c)",
    "((1: 1 1)) ((1: 1 2))", "a\nb", "(a b)\n(c d)", "(a b) (c d)", "''", "\"\"\"'\"`\"\"\"",
    "'x'''", "(name: 'with space' \"it's\")", "1\n2\n3", "() ()", "(a (b c) ((d)))", "'multi\nline'", "a\r",
};
foreach (var input in inputs)
{
    try
    {
        var links = new Parser().Parse(input);
        Console.WriteLine($"{input.Replace("\n", "\\n").Replace("\r", "\\r")} => [{string.Join(", ", links.Select(Show))}]");
    }
    catch (Exception e)
    {
        Console.WriteLine($"{input.Replace("\n", "\\n")} => ERR {e.GetType().Name}: {e.Message.Split('\n')[0]}");
    }
}
