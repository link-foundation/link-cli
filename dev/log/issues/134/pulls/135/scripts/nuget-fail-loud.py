"""Replace the silent NuGet skip in csharp.yml with an explicit failure (issue #134)."""
import pathlib, sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()

MISSING = (
    '            echo "::error title=NUGET_API_KEY is missing::The release was versioned but cannot be published to NuGet. '
    'Add a nuget.org API key with push access to the clink and Foundation.Data.Doublets.Cli packages as the NUGET_API_KEY '
    'repository secret (Settings > Secrets and variables > Actions), then re-run this job."\n'
    '            exit 1\n'
)

old_validate = (
    '          if [ -z "$NUGET_API_KEY" ]; then\n'
    '            echo "::warning::NUGET_API_KEY is not configured — NuGet publish will be skipped."\n'
    '            exit 0\n'
    '          fi\n'
)
new_validate = '          if [ -z "$NUGET_API_KEY" ]; then\n' + MISSING + '          fi\n'
assert text.count(old_validate) == 1
text = text.replace(old_validate, new_validate)

old_publish = (
    '          if [ -n "$NUGET_API_KEY" ]; then\n'
    '            dotnet nuget push ./artifacts/*.nupkg --api-key "$NUGET_API_KEY" --source https://api.nuget.org/v3/index.json --skip-duplicate\n'
    '            echo "published=true" >> "$GITHUB_OUTPUT"\n'
    '          else\n'
    '            echo "NUGET_API_KEY not set, skipping NuGet publish"\n'
    '            echo "published=false" >> "$GITHUB_OUTPUT"\n'
    '          fi\n'
)
new_publish = (
    '          # A missing key used to skip the push while the release stayed green\n'
    '          # (issue #134); a versioned but unpublished release must fail.\n'
    '          if [ -z "$NUGET_API_KEY" ]; then\n' + MISSING + '          fi\n'
    '          dotnet nuget push ./artifacts/*.nupkg --api-key "$NUGET_API_KEY" --source https://api.nuget.org/v3/index.json --skip-duplicate\n'
    '          echo "published=true" >> "$GITHUB_OUTPUT"\n'
)
assert text.count(old_publish) == 2
text = text.replace(old_publish, new_publish)

old_bump = (
    "      bump_type:\n"
    "        description: 'Version bump type'\n"
    "        required: true\n"
    "        type: choice\n"
    "        options:\n"
)
new_bump = (
    "      bump_type:\n"
    "        description: 'Version bump type'\n"
    "        required: true\n"
    "        type: choice\n"
    "        default: 'patch'\n"
    "        options:\n"
)
if old_bump in text:
    text = text.replace(old_bump, new_bump)
path.write_text(text)
print("updated", path)
