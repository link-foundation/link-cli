# TCP accept/shutdown race

Requirement mapping: R1/R7/R8/R12. Full Release validation reproduced a failure
in the existing finite `ShuttingDownRightAfterAConnectionArrivesIsClean` test.
The test starts, connects and stops 200 local servers; its failure is retained
in tcp-shutdown-before.txt with the actual exception and source line.

- [x] Preserve the automated before failure without increasing test timeouts.
- [x] Trace the runtime's listener shutdown/accept interleaving.
- [x] Handle the known null-reference exception only after shutdown starts.
- [x] Rerun all 2,027 C# tests in Debug and Release with coverage.
- [x] Publish the correction with its evidence and verify fresh exact-head CI.

The [.NET 10 listener source](https://github.com/dotnet/runtime/blob/v10.0.0/src/libraries/System.Net.Sockets/src/System/Net/Sockets/TCPListener.cs)
checks `_active` before using `_serverSocket` in `AcceptTcpClient`; `Stop`
disposes that socket and then clears the field. The captured stack is consistent
with stopping between the active check and the dereference. Existing catches
covered socket/disposal/stopped exceptions but omitted this interleaving.
The additional catch surrounds only the accept call and requires the server's
volatile stopping flag. An unexpected null reference before shutdown still
propagates. Ordinary shutdown and worker joining retain their existing flow.

Both complete C# configurations now pass, including the same 200-connection
regression. This is finite race evidence, not an exhaustive scheduling proof.
The broader issue and whole-project coverage criterion remain unfinished.

All nine workflows pass on published head `b125e6d`, including both native
language matrices. See inline-names-ci.txt for exact timestamps and links.
