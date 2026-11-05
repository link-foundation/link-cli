// Prints how Link.Foundation.Links.Notation parses the shapes the triplets
// query processor and the LiNo import accept (issue #151).
using Link.Foundation.Links.Notation;

string Describe(Link<string> link)
{
    var values = link.Values is null ? "null" : "[" + string.Join(", ", link.Values.Select(Describe)) + "]";
    return $"{{id={link.Id ?? "null"} values={values}}}";
}

var inputs = args.Length > 0 ? args : new[]
{
    "((i: T f t)) ((i: T f t2))",
    "(((i: T f t)) ((i: T f t2)))",
    "(() ((T f t)))",
    "(((i: T f t)) ())",
    "((($i: $t $f bob) ($t: * * *)) (($i: $t $f carol)))",
    "(1: 1 0 0)",
    "(1: 1 0 0)\n(2: 1 1 0)\n",
    "(1 0 0)",
    "(alice: Person 0 0)\n(Person 0 0)",
};
foreach (var input in inputs)
{
    Console.WriteLine(input.Replace("\n", "\\n"));
    try
    {
        foreach (var link in new Parser().Parse(input))
        {
            Console.WriteLine("  " + Describe(link));
        }
    }
    catch (Exception e)
    {
        Console.WriteLine("  error: " + e.Message);
    }
}
