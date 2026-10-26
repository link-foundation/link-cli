# Failed mapped-file growth

Requirement mapping: R1/R5/R7/R8/R9/R12. Preserve every wider criterion.

- [x] Trace the owned mapping and the installed memory-base capacity setter.
      The base publishes capacity only after a successful resize callback.
- [x] Reproduce rejected growth with an actual finite filesystem quota in an
      isolated child, retaining the old capacity, contents and durable flush.
- [x] Preserve the current mapping and write ownership until a replacement
      exists; release only newly acquired resources on failed initialization.
- [x] Check subsequent growth, following writes, exact file length, reopening,
      invalid-capacity rejection and repeated disposal on supported platforms.
- [x] Run focused and complete Debug/Release coverage, formatting, release and
      repository checks; record before/after evidence and commit atomically.
- [ ] Obtain fresh exact-head CI and continue every wider inventoried criterion.

The candidate is premature Unmap: a rejected SetLength leaves the base's old
capacity and pointer while disposing its mapping. Reproduction must precede a
production change. Limit the quota probe to three 4 KiB pages, ignore SIGXFSZ
only in its child, disable runtime memfd mappings, cap the child GC heap and
reap it on a finite deadline. A process exit is not physical power-loss proof.

The corrected before run has one genuine failure and one passing control:
after rejected growth, Flush throws ObjectDisposedException. See
mapped-growth-before.txt. Earlier test-oracle errors are recorded separately.
The replacement keeps the original stream and mapping alive through creation;
failed initialization disposes its newly opened stream. Smaller reservations
change the view and defer physical file truncation until used-length disposal.

Primary references: [installed base setter](https://github.com/linksplatform/Memory/blob/dd464b7b48a4efcaa90fcbaf0621944c184611fd/csharp/Platform.Memory/ResizableDirectMemoryBase.cs),
[Windows SetEndOfFile](https://learn.microsoft.com/en-us/windows/win32/api/fileapi/nf-fileapi-setendoffile),
[Windows mapping creation](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-createfilemappingw),
and [.NET Windows implementation](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.IO.MemoryMappedFiles/src/System/IO/MemoryMappedFiles/MemoryMappedFile.Windows.cs).
Mapping creation can extend the file; explicit SetLength with a live Windows
mapping would violate its API requirements. Platform CI must verify the change.

Focused contracts and both complete configurations pass: 2,126 tests each,
with no failures/skips. Complete measured lines reach 96.16% Debug and 96.84%
Release, still below the whole-project target. Formatting, release/source-size,
ShellCheck and 221 repository checks pass. See mapped-growth.txt for exact
reports. Fresh platform CI is required before accepting the native change.

Exact-head 1aafc31 C# CI passes Linux/macOS/Windows, including normal growth
and shrink/regrowth. Eight of nine workflows pass. Rust Windows run
40299771183 fails the pre-existing movement child deadline; preserved log lines
32629–32635 identify the failure. Its root cause remains under investigation;
this is not accepted as a passing CI batch. See mapped-growth-ci-before.txt.
