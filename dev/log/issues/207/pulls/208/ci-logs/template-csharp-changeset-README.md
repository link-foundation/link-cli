# Changesets

This folder is used by the changesets workflow to manage versioning and changelog updates.

## How to Add a Changeset

When making changes that affect the package version, create a changeset file:

```bash
# Create a new changeset file with a random name
touch .changeset/$(echo "my-change-$(date +%s)").md
```

Or create a file manually in `.changeset/` with any name ending in `.md` (except README.md).

## Changeset Format

Each changeset file must follow this format:

```markdown
---
'MyPackage': patch
---

Description of the changes made in this PR.
```

### Version Types

- `major` - Breaking changes (1.x.x -> 2.0.0)
- `minor` - New features, backwards compatible (x.1.x -> x.2.0)
- `patch` - Bug fixes, backwards compatible (x.x.1 -> x.x.2)

## Why Changesets?

This workflow, inspired by [Changesets](https://github.com/changesets/changesets):

1. **No merge conflicts** - Multiple PRs can add changesets independently
2. **Version safety** - Version bumps happen after PR merge, not before
3. **Per-PR documentation** - Each PR documents its own changes
4. **Automated releases** - Changesets are collected and versions are bumped automatically

## During Release

When changes are pushed to main:
1. All changeset files are collected
2. If multiple changesets exist, they are merged (using highest version bump)
3. Version is updated in the csproj file
4. CHANGELOG.md is updated with all descriptions
5. Package is published to NuGet
6. GitHub Release is created

## Manual Workflow

For manual releases, use `workflow_dispatch` with:
- `instant` mode - Immediate version bump and release
- `changeset-pr` mode - Create a PR with the changeset for review
