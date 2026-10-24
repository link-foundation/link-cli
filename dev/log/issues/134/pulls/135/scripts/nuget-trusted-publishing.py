"""Add NuGet trusted publishing (OIDC) with an API-key fallback to csharp.yml (issue #134)."""
import pathlib, re, sys

path = pathlib.Path(sys.argv[1])
text = path.read_text()

old_msg = ('echo "::error title=NUGET_API_KEY is missing::The release was versioned but cannot be published to NuGet. '
           'Add a nuget.org API key with push access to the clink and Foundation.Data.Doublets.Cli packages as the NUGET_API_KEY '
           'repository secret (Settings > Secrets and variables > Actions), then re-run this job."')
new_msg = ('echo "::error title=NuGet credentials are missing::The release was versioned but cannot be published to NuGet. '
           'Either configure a nuget.org trusted publishing policy for this repository and csharp.yml and set the NUGET_USER '
           'repository variable to the nuget.org account name, or add a nuget.org API key with push access to the clink and '
           'Foundation.Data.Doublets.Cli packages as the NUGET_API_KEY repository secret (Settings > Secrets and variables > '
           'Actions), then re-run this job."')
assert text.count(old_msg) == 3
text = text.replace(old_msg, new_msg)

# 1. OIDC token for both publishing jobs.
old_perm = "    permissions:\n      contents: write\n      packages: write\n    steps:\n"
assert text.count(old_perm) == 2
text = text.replace(old_perm, "    permissions:\n      contents: write\n      packages: write\n      # NuGet trusted publishing (NuGet/login) exchanges the OIDC token.\n      id-token: write\n    steps:\n")

# 2. Validation accepts trusted publishing.
old_validate_env = ("        env:\n          NUGET_API_KEY: ${{ secrets.NUGET_API_KEY }}\n        run: |\n"
                    "          if [ -z \"$NUGET_API_KEY\" ]; then\n")
assert text.count(old_validate_env) == 1
text = text.replace(old_validate_env,
                    "        env:\n          NUGET_API_KEY: ${{ secrets.NUGET_API_KEY }}\n          NUGET_USER: ${{ vars.NUGET_USER }}\n        run: |\n"
                    "          if [ -n \"$NUGET_USER\" ]; then\n"
                    "            echo \"Publishing with NuGet trusted publishing as $NUGET_USER\"\n"
                    "            exit 0\n"
                    "          fi\n"
                    "          if [ -z \"$NUGET_API_KEY\" ]; then\n")

# 3. Login step before each publish step, reusing that step's condition.
pattern = re.compile(
    r"(      - name: Publish to NuGet\n        id: nuget_publish\n        if: >-\n((?:          .*\n)+?))"
    r"        env:\n          NUGET_API_KEY: \$\{\{ secrets\.NUGET_API_KEY \}\}\n")
def login(match):
    condition = match.group(2)
    lines = condition.splitlines()
    guarded = "          vars.NUGET_USER != '' && (\n" + "\n".join(lines) + ")\n"
    return (
        "      - name: NuGet login (OIDC)\n"
        "        id: nuget-login\n"
        "        # Trusted publishing: a one-hour key requested right before the push.\n"
        "        if: >-\n" + guarded +
        "        uses: NuGet/login@8d196754b4036150537f80ac539e15c2f1028841 # v1.2.0\n"
        "        with:\n"
        "          user: ${{ vars.NUGET_USER }}\n\n"
        + match.group(1) +
        "        env:\n"
        "          # The long-lived secret is only the fallback while trusted\n"
        "          # publishing is not configured (vars.NUGET_USER unset).\n"
        "          NUGET_API_KEY: ${{ vars.NUGET_USER == '' && secrets.NUGET_API_KEY || steps.nuget-login.outputs.NUGET_API_KEY }}\n"
    )
text, count = pattern.subn(login, text)
assert count == 2, count

old_push = '          dotnet nuget push ./artifacts/*.nupkg --api-key "$NUGET_API_KEY" --source https://api.nuget.org/v3/index.json --skip-duplicate\n'
assert text.count(old_push) == 2
text = text.replace(old_push,
    '          # zizmor flags every --api-key push; the key is the short-lived\n'
    '          # trusted-publishing key whenever vars.NUGET_USER is set.\n'
    '          dotnet nuget push ./artifacts/*.nupkg --api-key "$NUGET_API_KEY" --source https://api.nuget.org/v3/index.json --skip-duplicate # zizmor: ignore[use-trusted-publishing]\n')
path.write_text(text)
print("updated", path)
