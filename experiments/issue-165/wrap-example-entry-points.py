from pathlib import Path

programs = [
    ("examples/debug_changes.cs", "Clink.Examples.DebugChanges"),
    ("examples/test_issue_scenario.cs", "Clink.Examples.IssueScenario"),
    ("examples/embedded-store/csharp/Program.cs", "Clink.Examples.EmbeddedStore"),
    ("examples/lino-structure-probe/Program.cs", "Clink.Examples.LinoStructure"),
    (
        "experiments/issue-151/csharp-parser-shapes/Program.cs",
        "Clink.Experiments.ParserShapes",
    ),
]
for filename, namespace in programs:
    path = Path(filename)
    text = path.read_text()
    lines = text.splitlines()
    first = next(i for i, line in enumerate(lines) if line.startswith("using "))
    end = first
    while end < len(lines) and (
        lines[end].startswith("using ")
        or lines[end].startswith("//")
        or not lines[end].strip()
    ):
        end += 1
    header = "\n".join(lines[:end]).rstrip()
    body = "\n".join(lines[end:])
    tail = ""
    if "internal sealed class Scope" in body:
        body, tail = body.split("/// <summary>Owns", 1)
        tail = "\n/// <summary>Owns" + tail
    if filename == "examples/debug_changes.cs":
        header = (
            "using System;\nusing System.Collections.Generic;\nusing System.Linq;\n"
            + header
        )
    indented = "\n".join(
        "        " + line if line else "" for line in body.rstrip().splitlines()
    )
    path.write_text(
        header
        + "\n\nnamespace "
        + namespace
        + ";\n\n// Explicit entry points keep independent examples analyzable together by CodeQL.\ninternal static class Program\n{\n    private static void Main(string[] args)\n    {\n"
        + indented
        + "\n    }\n}\n"
        + tail.rstrip()
        + "\n"
    )
