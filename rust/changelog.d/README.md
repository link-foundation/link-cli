# Changelog Fragments

This folder contains changelog fragment files for the Rust package.

## What is a changelog fragment?

A changelog fragment is a markdown file that describes a change. When merged to main, these files are consumed by the release workflow to:

1. Determine the version bump type (major, minor, patch)
2. Generate changelog entries
3. Create releases

## How to create a changelog fragment

Create a new markdown file in this directory with a unique name (e.g., `YYYYMMDD_HHMMSS_description.md`) with the following format:

```markdown
---
bump: minor
---

Added new feature that does X, Y, and Z.
```

The `bump` field in the frontmatter should be:
- `major` - Breaking changes
- `minor` - New features (backwards compatible)
- `patch` - Bug fixes (backwards compatible)

The content after the second `---` is the changelog description.

## Naming convention

Recommended naming format: `YYYYMMDD_HHMMSS_short_description.md`

Example: `20251231_120000_add_new_feature.md`

## Multiple fragments

If your PR has multiple logical changes, you can create multiple fragment files. They will be merged automatically during release.
