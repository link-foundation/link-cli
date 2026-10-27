#!/usr/bin/env bash
# Preserve the unchanged compound setup operation in isolated baseline source.
set -euo pipefail
cd "$(dirname "$0")/../.."
baseline=950bcb34ed060674ed3896f1deb187706be62c09
probe_dir=$(mktemp -d "$PWD/experiments/issue-138/version-setup-baseline.XXXXXX")
git archive "$baseline" csharp docs/protocol | tar -x -C "$probe_dir"
cp csharp/Foundation.Data.Doublets.Cli.Tests/VersionExitSetupTests.cs "$probe_dir/csharp/Foundation.Data.Doublets.Cli.Tests/"
# Extracting the original call enables observing it without changing its effects.
python3 - "$probe_dir" <<'PYCODE'
import sys
from pathlib import Path
p=Path(sys.argv[1])/'csharp/Foundation.Data.Doublets.Cli.Tests/VersionExitProbe.cs'
s=p.read_text()
s=s.replace('    internal static int Run(', '    internal static uint CreateInitialRow(INamedTypesLinks<uint> store, Action<string> phase)\n    {\n        var id = store.CreateAndUpdate(11u, 12u);\n        phase("row-created");\n        return id;\n    }\n\n    internal static int Run(')
p.write_text(s)
PYCODE
export DOTNET_ROOT="$PWD/experiments/issue-138/tools/dotnet-10.0.403"
export PATH="$DOTNET_ROOT:$PATH"
export DOTNET_GCHeapHardLimit=0x28000000
set +e
dotnet test "$probe_dir/csharp/Foundation.Data.Doublets.Cli.Tests" --configuration Release --disable-build-servers -m:1 --filter FullyQualifiedName~VersionExitSetupTests > "$probe_dir/tests.log" 2>&1
result=$?
set -e
test "$result" -eq 1
rg -q 'Failed: *1, Passed: *1, Skipped: *0, Total: *2' "$probe_dir/tests.log"
rg -q 'row-allocated' "$probe_dir/tests.log"
printf 'PASS immutable %s combined CreateAndUpdate omits completed-creation progress; logs: %s\n' "$baseline" "$probe_dir"
