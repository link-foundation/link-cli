#!/usr/bin/env rust-script
//! Check if a release is needed based on changelog fragments and version state
//!
//! This script checks:
//! 1. If there are changelog fragments to process
//! 2. If the current version has already been published to crates.io
//! 3. If the matching GitHub release and configured Docker Hub image tag exist
//!
//! IMPORTANT: This script checks external release artifacts, NOT git tags.
//! This is critical because:
//! - Git tags can exist without the package being published
//! - GitHub releases create tags but do not publish to crates.io or Docker Hub
//! - A crates.io publish can succeed while later Docker/GitHub release steps fail
//!
//! Supports both single-language and multi-language repository structures:
//! - Single-language: Cargo.toml in repository root
//! - Multi-language: Cargo.toml in rust/ subfolder
//!
//! Usage: rust-script rust/scripts/check-release-needed.rs [--rust-root <path>]
//!
//! Environment variables:
//!   - HAS_FRAGMENTS: 'true' if changelog fragments exist (from get-bump-type.rs)
//!   - DOCKERHUB_IMAGE: Optional Docker Hub image name to verify (namespace/repository)
//!   - GITHUB_REPOSITORY: GitHub repository to verify (owner/repository)
//!
//! Outputs (written to GITHUB_OUTPUT):
//!   - should_release: 'true' if a release should be created
//!   - skip_bump: 'true' if version bump should be skipped while missing artifacts are recreated
//!   - crate_published: 'true' if the current version already exists on crates.io
//!   - dockerhub_required: 'true' if Docker Hub publishing is configured and a Dockerfile exists
//!   - dockerhub_published: 'true' if the configured Docker Hub tag exists
//!   - github_release_published: 'true' if the matching GitHub release exists
//!   - max_published_version: the highest non-yanked version on crates.io (for downstream use)
//!
//! ```cargo
//! [dependencies]
//! regex = "1"
//! ureq = "2"
//! serde = { version = "1", features = ["derive"] }
//! serde_json = "1"
//! ```

use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;

#[path = "rust-paths.rs"]
mod rust_paths;

#[path = "registry.rs"]
mod registry;

fn get_arg(name: &str) -> Option<String> {
    let args: Vec<String> = env::args().collect();
    let flag = format!("--{}", name);

    if let Some(idx) = args.iter().position(|a| a == &flag) {
        return args.get(idx + 1).cloned();
    }

    let env_name = name.to_uppercase().replace('-', "_");
    env::var(&env_name).ok().filter(|s| !s.is_empty())
}

fn set_output(key: &str, value: &str) {
    if let Ok(output_file) = env::var("GITHUB_OUTPUT") {
        if let Err(e) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&output_file)
            .and_then(|mut f| {
                use std::io::Write;
                writeln!(f, "{}={}", key, value)
            })
        {
            eprintln!("Warning: Could not write to GITHUB_OUTPUT: {}", e);
        }
    }
    println!("Output: {}={}", key, value);
}

/// Stop the job when a registry cannot be asked: reading a failed lookup as
/// "not published" would release a published version again.
fn or_exit<T>(lookup: &str, result: Result<T, String>) -> T {
    result.unwrap_or_else(|e| {
        eprintln!("::error title={} failed::{}", lookup, e);
        exit(1);
    })
}

fn split_docker_image(image: &str) -> Option<(&str, &str)> {
    let mut parts = image.split('/');
    let namespace = parts.next()?;
    let repository = parts.next()?;

    if parts.next().is_some() || namespace.is_empty() || repository.is_empty() {
        None
    } else {
        Some((namespace, repository))
    }
}

fn check_docker_hub_tag(image: &str, version: &str) -> Result<bool, String> {
    let (namespace, repository) = split_docker_image(image).ok_or_else(|| {
        format!(
            "Could not parse Docker Hub image '{}'; expected namespace/repository",
            image
        )
    })?;
    let url = format!(
        "https://hub.docker.com/v2/repositories/{}/{}/tags/{}",
        namespace, repository, version
    );
    Ok(registry::get_body(&url, &[])?.is_some())
}

fn docker_hub_image_to_check() -> Option<String> {
    get_arg("dockerhub-image")
        .or_else(|| get_arg("docker-hub-image"))
        .or_else(|| get_arg("dockerhub_image"))
        .filter(|image| Path::new("Dockerfile").exists() && !image.trim().is_empty())
}

fn release_is_complete(
    crate_published: bool,
    dockerhub_required: bool,
    dockerhub_published: bool,
    github_release_published: bool,
) -> bool {
    crate_published && (!dockerhub_required || dockerhub_published) && github_release_published
}

fn main() {
    let rust_root = match rust_paths::get_rust_root(None, true) {
        Ok(root) => root,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };
    let cargo_toml = rust_paths::get_cargo_toml_path(&rust_root);
    let package_manifest = match rust_paths::get_package_manifest_path(&cargo_toml) {
        Ok(path) => path,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };

    let has_fragments = env::var("HAS_FRAGMENTS")
        .map(|v| v == "true")
        .unwrap_or(false);

    let package_info = match rust_paths::read_package_info(&package_manifest) {
        Ok(info) => info,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };
    let crate_name = package_info.name;
    let current_version = package_info.version;

    let max_published = or_exit(
        "crates.io lookup",
        registry::max_published_version(&crate_name),
    );
    if let Some((major, minor, patch)) = max_published {
        let max_ver = format!("{}.{}.{}", major, minor, patch);
        println!("Max published version on crates.io: {}", max_ver);
        set_output("max_published_version", &max_ver);
    } else {
        println!("No versions published on crates.io yet (or crate not found)");
        set_output("max_published_version", "");
    }

    if !has_fragments {
        let crate_published = or_exit(
            "crates.io lookup",
            registry::version_published(&crate_name, &current_version),
        );
        let tag_prefix = get_arg("tag-prefix").unwrap_or_else(|| "v".to_string());
        let dockerhub_image = docker_hub_image_to_check();
        let dockerhub_required = dockerhub_image.is_some();
        let dockerhub_published = dockerhub_image
            .as_deref()
            .map(|image| {
                or_exit(
                    "Docker Hub lookup",
                    check_docker_hub_tag(image, &current_version),
                ) && or_exit("Docker Hub lookup", check_docker_hub_tag(image, "latest"))
            })
            .unwrap_or(false);
        let github_release_published = get_arg("repository")
            .or_else(|| env::var("GITHUB_REPOSITORY").ok().filter(|s| !s.is_empty()))
            .map(|repository| {
                let tag = format!("{}{}", tag_prefix, current_version);
                or_exit(
                    "GitHub release lookup",
                    registry::github_release_exists(&repository, &tag),
                )
            })
            .unwrap_or_else(|| {
                eprintln!("Warning: GITHUB_REPOSITORY not set; assuming GitHub release is missing");
                false
            });

        set_output(
            "crate_published",
            if crate_published { "true" } else { "false" },
        );
        set_output(
            "dockerhub_required",
            if dockerhub_required { "true" } else { "false" },
        );
        set_output(
            "dockerhub_published",
            if dockerhub_published { "true" } else { "false" },
        );
        set_output(
            "github_release_published",
            if github_release_published {
                "true"
            } else {
                "false"
            },
        );

        println!(
            "Crate: {}, Version: {}, Published on crates.io: {}",
            crate_name, current_version, crate_published
        );
        if let Some(image) = dockerhub_image {
            println!(
                "Docker image: {}, version/latest tags published on Docker Hub: {}",
                image, dockerhub_published
            );
        } else {
            println!("Docker Hub artifact check skipped: DOCKERHUB_IMAGE or Dockerfile is not configured");
        }
        println!(
            "GitHub release {}{} published: {}",
            tag_prefix, current_version, github_release_published
        );

        if release_is_complete(
            crate_published,
            dockerhub_required,
            dockerhub_published,
            github_release_published,
        ) {
            println!(
                "No changelog fragments and v{} is fully published",
                current_version
            );
            set_output("should_release", "false");
        } else {
            println!(
                "No changelog fragments but v{} is missing at least one release artifact",
                current_version
            );
            set_output("should_release", "true");
            set_output("skip_bump", "true");
        }
    } else {
        println!("Found changelog fragments, proceeding with release");
        set_output("should_release", "true");
        set_output("skip_bump", "false");
    }
}
