Files copied verbatim from https://github.com/NuGet/NuGetGallery (default branch, fetched 2026-11-10) to show how the
gallery answers an API key without the required scope (403 from `ApiScopeRequiredAttribute`/`ApiController`), which
the release authentication preflight must report as a key-scope problem, not as an unreachable registry.
