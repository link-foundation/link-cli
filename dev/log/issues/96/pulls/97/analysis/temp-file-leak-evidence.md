# Evidence: temp-file leak fixed (measurable on Linux)

The Windows `IOException`s are the visible symptom of a leak that also existed on Linux —
it was simply invisible there, because POSIX allows unlinking a still-mapped file, and
because several test helpers never deleted the `.names.links` companion database at all.

Reproduction, before the fix (`dotnet test --no-build -c Release`, Linux):

```
$ ls /tmp/*.names.links | wc -l
104
```

After the fix (temp directory cleared, full suite re-run):

```
$ rm -f /tmp/*.names.links
$ dotnet test --no-build -c Release
Passed!  - Failed: 0, Passed: 222, Skipped: 0, Total: 222
$ ls /tmp/*.names.links 2>/dev/null | wc -l
0
```

Every names database is now released and deleted. On Windows the same change is what makes
the `File.Delete` calls succeed instead of throwing.
