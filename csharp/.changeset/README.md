# Changesets

This folder contains changeset files for the C# package.

## What is a changeset?

A changeset is a markdown file that describes a version bump and contains the change description. When merged to main, these files are consumed by the release workflow to:

1. Determine the version bump type (major, minor, patch)
2. Generate changelog entries
3. Create releases

## How to create a changeset

Create a new markdown file in this directory with a unique name (e.g., `add-new-feature.md`) with the following format:

```markdown
---
'Foundation.Data.Doublets.Cli': minor
---

Added new feature that does X, Y, and Z.
```

The first line after `---` should be the package name followed by the bump type:
- `major` - Breaking changes
- `minor` - New features (backwards compatible)
- `patch` - Bug fixes (backwards compatible)

The content after the second `---` is the changelog description.

## Multiple changesets

If your PR has multiple logical changes, you can create multiple changeset files. They will be merged automatically during release.
