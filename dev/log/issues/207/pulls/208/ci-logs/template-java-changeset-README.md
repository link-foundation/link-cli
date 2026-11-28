# Changesets

This directory contains changeset files that document changes for upcoming releases.

## Why Changesets?

Using individual changeset files instead of editing `CHANGELOG.md` directly:
- **Avoids merge conflicts** when multiple PRs are open
- **Associates changes with PRs** for better traceability
- **Enforces documentation** of all changes
- **Determines version bump type** automatically from the changesets

## Creating a Changeset

### Using the Script

```bash
# Create a new changeset
bun scripts/create-manual-changeset.mjs --bump-type patch --description "Fix a bug"
bun scripts/create-manual-changeset.mjs --bump-type minor --description "Add new feature"
bun scripts/create-manual-changeset.mjs --bump-type major --description "Breaking change"
```

### Manually

Create a file with a random name (e.g., `happy-cats-dance.md`) in this directory with the following format:

```markdown
---
'java-ai-driven-development-pipeline-template': patch
---

Description of the changes made.
```

## Changeset Format

Each changeset file has two parts:

1. **Frontmatter** (between `---` markers):
   - Package name and version bump type (major, minor, or patch)
2. **Description**:
   - Clear description of what changed
   - Use present tense ("Add feature" not "Added feature")
   - Reference issue numbers when applicable: "Fix login bug (#123)"

### Version Bump Types

- **patch**: Bug fixes, documentation updates, internal changes
- **minor**: New features, non-breaking enhancements
- **major**: Breaking changes, API changes

## Example Changeset

```markdown
---
'java-ai-driven-development-pipeline-template': minor
---

Add async delay function with CompletableFuture support for non-blocking operations.
```

## What Happens During Release

1. The release workflow detects pending changesets
2. Multiple changesets are merged (highest bump type wins)
3. Version is bumped according to the determined bump type
4. A new version entry is added to `CHANGELOG.md`
5. Changeset files are deleted
6. Changes are committed and pushed to main

## Tips

- One changeset per PR is typical
- Be concise but descriptive in your descriptions
- The bump type determines the version number change
- Multiple changesets are automatically merged during release
