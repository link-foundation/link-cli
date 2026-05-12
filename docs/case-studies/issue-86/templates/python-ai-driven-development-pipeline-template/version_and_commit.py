#!/usr/bin/env python3
"""
Version packages and commit to main branch.

This script handles version bumping and committing for CI/CD workflows.
It supports idempotent re-runs and detects when work was already completed.

Usage:
    python scripts/version_and_commit.py --bump-type <major|minor|patch> [--description "..."]

Example:
    python scripts/version_and_commit.py --bump-type patch
    python scripts/version_and_commit.py --bump-type minor --description "New feature"

Environment variables:
    GITHUB_OUTPUT: Path to GitHub Actions output file
"""

import argparse
import os
import re
import subprocess
import sys
from pathlib import Path


def run_command(
    cmd: list[str], check: bool = True, capture: bool = False
) -> subprocess.CompletedProcess:
    """Run a command and handle errors."""
    cmd_str = " ".join(cmd)
    print(f"Running: {cmd_str}")

    result = subprocess.run(
        cmd,
        capture_output=capture,
        text=True,
        check=False,
    )

    if not capture:
        if result.stdout:
            print(result.stdout)
        if result.stderr:
            print(result.stderr, file=sys.stderr)

    if check and result.returncode != 0:
        if capture:
            print(result.stdout)
            print(result.stderr, file=sys.stderr)
        print(
            f"Error: Command failed with exit code {result.returncode}",
            file=sys.stderr,
        )
        sys.exit(result.returncode)

    return result


def set_github_output(key: str, value: str) -> None:
    """Set GitHub Actions output variable."""
    output_file = os.environ.get("GITHUB_OUTPUT")
    if output_file:
        with open(output_file, "a") as f:
            f.write(f"{key}={value}\n")
        print(f"Set output: {key}={value}")


def get_current_version(pyproject_path: Path) -> str:
    """Get version from pyproject.toml."""
    content = pyproject_path.read_text()
    match = re.search(r'^version\s*=\s*["\']([^"\']+)["\']', content, re.MULTILINE)
    if not match:
        raise ValueError("Could not find version in pyproject.toml")
    return match.group(1)


def configure_git() -> None:
    """Configure git for automated commits."""
    print("Configuring git...")
    run_command(
        ["git", "config", "user.name", "github-actions[bot]"],
    )
    run_command(
        ["git", "config", "user.email", "github-actions[bot]@users.noreply.github.com"],
    )


def check_remote_changes(pyproject_path: Path) -> tuple[bool, str]:
    """
    Check if remote main has advanced (handles re-runs).
    Returns (already_released, remote_version).
    """
    print("\nChecking for remote changes...")
    run_command(["git", "fetch", "origin", "main"])

    # Get commit SHAs
    local_head = run_command(
        ["git", "rev-parse", "HEAD"],
        capture=True,
    ).stdout.strip()

    remote_head = run_command(
        ["git", "rev-parse", "origin/main"],
        capture=True,
    ).stdout.strip()

    if local_head != remote_head:
        print(f"Remote main has advanced (local: {local_head}, remote: {remote_head})")
        print("This may indicate a previous attempt partially succeeded.")

        # Get remote version
        remote_content = run_command(
            ["git", "show", "origin/main:pyproject.toml"],
            capture=True,
        ).stdout

        remote_match = re.search(
            r'^version\s*=\s*["\']([^"\']+)["\']',
            remote_content,
            re.MULTILINE,
        )
        if remote_match:
            remote_version = remote_match.group(1)
            print(f"Remote version: {remote_version}")

            # Check if versions differ (indicating work was done)
            local_version = get_current_version(pyproject_path)
            if local_version != remote_version:
                print("Local and remote versions differ, rebasing...")
                run_command(["git", "rebase", "origin/main"])
                return False, remote_version
            else:
                print("Versions match, assuming previous run completed successfully")
                return True, remote_version

    return False, ""


def main() -> int:
    """Main entry point."""
    parser = argparse.ArgumentParser(
        description="Version bump and commit for CI/CD",
    )
    parser.add_argument(
        "--bump-type",
        choices=["major", "minor", "patch"],
        required=True,
        help="Type of version bump",
    )
    parser.add_argument(
        "--description",
        default="",
        help="Description for changelog",
    )

    args = parser.parse_args()

    # Determine project root
    script_dir = Path(__file__).parent
    project_root = script_dir.parent
    pyproject_path = project_root / "pyproject.toml"

    if not pyproject_path.exists():
        print(f"Error: {pyproject_path} not found", file=sys.stderr)
        return 1

    try:
        # Configure git
        configure_git()

        # Check for remote changes
        already_released, remote_version = check_remote_changes(pyproject_path)

        if already_released:
            print("Version bump already completed in previous run")
            set_github_output("version_committed", "false")
            set_github_output("already_released", "true")
            set_github_output("new_version", remote_version)
            return 0

        # Get current version
        old_version = get_current_version(pyproject_path)
        print(f"\nCurrent version: {old_version}")

        # Run version bump
        print(f"\nBumping version ({args.bump_type})...")
        bump_cmd = [
            sys.executable,
            "scripts/bump_version.py",
            args.bump_type,
        ]
        if args.description:
            bump_cmd.extend(["--description", args.description])

        run_command(bump_cmd)

        # Get new version
        new_version = get_current_version(pyproject_path)
        print(f"New version: {new_version}")
        set_github_output("new_version", new_version)

        # Check for changes
        status = run_command(
            ["git", "status", "--porcelain"],
            capture=True,
        ).stdout.strip()

        if status:
            print("\nChanges detected, committing...")

            # Stage all changes
            run_command(["git", "add", "-A"])

            # Commit with version as message
            run_command(["git", "commit", "-m", new_version])

            # Push to main
            run_command(["git", "push", "origin", "main"])

            print(
                f"\n✅ Version bump committed and pushed: {old_version} → {new_version}"
            )
            set_github_output("version_committed", "true")
        else:
            print("\nNo changes to commit")
            set_github_output("version_committed", "false")

        return 0

    except Exception as e:
        print(f"Error: {e}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
