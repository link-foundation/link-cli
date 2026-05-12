# csharp-ai-driven-development-pipeline-template

A comprehensive template for AI-driven C# development with full CI/CD pipeline support.

[![CI/CD Pipeline](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/workflows/CI%2FCD%20Pipeline/badge.svg)](https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template/actions)
[![.NET Version](https://img.shields.io/badge/.NET-8.0-blue.svg)](https://dotnet.microsoft.com/)
[![License: Unlicense](https://img.shields.io/badge/license-Unlicense-blue.svg)](http://unlicense.org/)

## Features

- **.NET 8.0 support**: Works with the latest .NET LTS version
- **Cross-platform testing**: CI runs on Ubuntu, macOS, and Windows
- **Comprehensive testing**: xUnit tests with coverage reporting
- **Code quality**: EditorConfig + .NET analyzers with warnings as errors
- **Pre-commit hooks**: Automated code quality checks before commits
- **CI/CD pipeline**: GitHub Actions with multi-platform support
- **Changesets workflow**: Version-safe changelog management (like JavaScript Changesets)
- **Release automation**: Automatic NuGet publishing and GitHub releases

## Quick Start

### Using This Template

1. Click "Use this template" on GitHub to create a new repository
2. Clone your new repository
3. Update `src/MyPackage/MyPackage.csproj` with your package name and description
4. Rename the solution and project files as needed
5. Update imports in tests and examples
6. Build and start developing!

### Development Setup

```bash
# Clone the repository
git clone https://github.com/link-foundation/csharp-ai-driven-development-pipeline-template.git
cd csharp-ai-driven-development-pipeline-template

# Build the project
dotnet build

# Run tests
dotnet test

# Run the example
dotnet run --project examples/BasicUsage
```

### Running Tests

```bash
# Run all tests
dotnet test

# Run tests with verbose output
dotnet test --verbosity normal

# Run tests with coverage
dotnet test --collect:"XPlat Code Coverage"

# Run a specific test
dotnet test --filter "FullyQualifiedName~CalculatorTests"
```

### Code Quality Checks

```bash
# Format code
dotnet format

# Check formatting (CI style)
dotnet format --verify-no-changes

# Build with warnings as errors
dotnet build --configuration Release /warnaserror

# Check file size limits
bun run scripts/check-file-size.mjs

# Run all checks
dotnet format --verify-no-changes && dotnet build --configuration Release /warnaserror && bun run scripts/check-file-size.mjs
```

## Project Structure

```
.
├── .changeset/                 # Changesets configuration
│   ├── config.json             # Changeset settings
│   ├── README.md               # Changeset instructions
│   └── *.md                    # Individual changesets
├── .github/
│   └── workflows/
│       └── release.yml         # CI/CD pipeline configuration
├── examples/
│   ├── BasicUsage.cs           # Usage example
│   └── BasicUsage.csproj       # Example project
├── scripts/
│   ├── bump-version.mjs        # Version bumping utility
│   ├── check-file-size.mjs     # File size validation script
│   ├── create-github-release.mjs # GitHub release creation
│   ├── merge-changesets.mjs    # Merge multiple changesets
│   ├── validate-changeset.mjs  # PR changeset validation
│   └── version-and-commit.mjs  # CI/CD version management
├── src/
│   └── MyPackage/
│       ├── Calculator.cs       # Example implementation
│       ├── PackageInfo.cs      # Package version info
│       └── MyPackage.csproj    # Library project
├── tests/
│   └── MyPackage.Tests/
│       ├── CalculatorTests.cs  # Test suite
│       ├── PackageInfoTests.cs # Package info tests
│       └── MyPackage.Tests.csproj # Test project
├── .editorconfig               # Code style configuration
├── .gitignore                  # Git ignore patterns
├── .pre-commit-config.yaml     # Pre-commit hooks configuration
├── Directory.Build.props       # Shared build properties
├── MyPackage.sln               # Solution file
├── CHANGELOG.md                # Project changelog
├── CONTRIBUTING.md             # Contribution guidelines
├── LICENSE                     # Unlicense (public domain)
└── README.md                   # This file
```

## Design Choices

### Code Quality Tools

- **EditorConfig**: Consistent code style across all editors
  - Enforces naming conventions, formatting, and style rules
  - Integrated with .NET analyzers

- **.NET Analyzers**: Built-in static analysis
  - All warnings treated as errors
  - Latest analysis level enabled
  - Enforces best practices

- **Pre-commit hooks**: Automated checks before each commit
  - Runs dotnet format to ensure formatting
  - Builds with warnings as errors
  - Runs tests to prevent broken commits

### Testing Strategy

The template supports multiple levels of testing:

- **Unit tests**: In `tests/MyPackage.Tests/` using xUnit
- **Theory tests**: Data-driven tests with `[Theory]` and `[InlineData]`
- **Coverage**: Automatic collection with Coverlet
- **Examples**: In `examples/` directory (also serve as documentation)

### Changesets Workflow

This template uses a changesets workflow similar to [Changesets](https://github.com/changesets/changesets) in JavaScript:

Benefits:
- **No merge conflicts**: Multiple PRs can add changesets independently
- **Version safety**: Version bumps happen after PR merge, not before
- **Per-PR documentation**: Each PR documents its own changes
- **Automated releases**: Changesets are collected and processed automatically

```bash
# Create a changeset file
cat > .changeset/my-change.md << 'EOF'
---
'MyPackage': patch
---

Description of your changes
EOF
```

Version types:
- `major` - Breaking changes (1.x.x -> 2.0.0)
- `minor` - New features (x.1.x -> x.2.0)
- `patch` - Bug fixes (x.x.1 -> x.x.2)

### CI/CD Pipeline

The GitHub Actions workflow provides:

1. **Changeset validation**: Ensures PRs include a changeset file
2. **Linting**: dotnet format and build with warnings as errors
3. **Test matrix**: 3 OS (Ubuntu, macOS, Windows) with .NET 8.0
4. **Building**: Release build and package validation
5. **Release**: Automated versioning, NuGet publishing, and GitHub releases

### Release Automation

The release workflow supports two modes:

**Automatic Release** (on push to main):
1. Detects changesets in `.changeset/` directory
2. Merges multiple changesets if needed (using highest bump type)
3. Updates version in csproj and CHANGELOG.md
4. Creates git tag and pushes changes
5. Publishes to NuGet and creates GitHub release

**Manual Release** (via workflow_dispatch):
- `instant` mode: Immediate version bump and release
- `changeset-pr` mode: Creates a PR with changeset for review

## Configuration

### Updating Package Name

After creating a repository from this template:

1. Update `src/MyPackage/MyPackage.csproj`:
   - Change `PackageId` field
   - Update `RepositoryUrl`
   - Change description and authors

2. Rename the solution and project files:
   - `MyPackage.sln`
   - `src/MyPackage/`
   - `tests/MyPackage.Tests/`

3. Update project references in:
   - `examples/BasicUsage.csproj`
   - `tests/MyPackage.Tests/MyPackage.Tests.csproj`

4. Update imports in source files

### EditorConfig

Code style is configured in `.editorconfig`. Current configuration:

- 4-space indentation for C# files
- LF line endings
- File-scoped namespaces
- Expression-bodied members preferred
- var preferred where type is apparent

### Analyzer Configuration

Analyzers are configured in `Directory.Build.props`:

- All warnings treated as errors
- Latest analysis level enabled
- .NET analyzers enabled
- Code style enforcement in build

## Scripts Reference

| Script                              | Description                    |
| ----------------------------------- | ------------------------------ |
| `dotnet test`                       | Run all tests                  |
| `dotnet format`                     | Format code                    |
| `dotnet build /warnaserror`         | Build with strict warnings     |
| `dotnet run --project examples/BasicUsage` | Run example             |
| `bun run scripts/check-file-size.mjs` | Check file size limits       |
| `bun run scripts/bump-version.mjs`  | Bump version                   |

## Example Usage

```csharp
using MyPackage;

// Basic arithmetic
var sum = Calculator.Add(2, 3);       // 5
var product = Calculator.Multiply(2, 3);  // 6

Console.WriteLine($"2 + 3 = {sum}");
Console.WriteLine($"2 * 3 = {product}");

// Async operations
await Calculator.DelayAsync(1.0);  // Wait for 1 second
```

See `examples/BasicUsage.cs` for more examples.

## Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Workflow

1. Fork the repository
2. Create a feature branch: `git checkout -b feature/my-feature`
3. Make your changes and add tests
4. Run quality checks: `dotnet format && dotnet build /warnaserror && dotnet test`
5. Add a changeset file in `.changeset/`
6. Commit your changes (pre-commit hooks will run automatically)
7. Push and create a Pull Request

## License

[Unlicense](LICENSE) - Public Domain

This is free and unencumbered software released into the public domain. See [LICENSE](LICENSE) for details.

## Acknowledgments

Inspired by:
- [js-ai-driven-development-pipeline-template](https://github.com/link-foundation/js-ai-driven-development-pipeline-template)
- [python-ai-driven-development-pipeline-template](https://github.com/link-foundation/python-ai-driven-development-pipeline-template)
- [rust-ai-driven-development-pipeline-template](https://github.com/link-foundation/rust-ai-driven-development-pipeline-template)

## Resources

- [.NET Documentation](https://docs.microsoft.com/dotnet/)
- [xUnit Documentation](https://xunit.net/)
- [EditorConfig Documentation](https://editorconfig.org/)
- [.NET Analyzers](https://docs.microsoft.com/dotnet/fundamentals/code-analysis/overview)
- [Pre-commit Documentation](https://pre-commit.com/)
