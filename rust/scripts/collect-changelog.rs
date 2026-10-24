#!/usr/bin/env rust-script
//! Collect changelog fragments into CHANGELOG.md
//!
//! This script collects all .md files from changelog.d/ (except README.md)
//! and prepends them to CHANGELOG.md, then removes the processed fragments.
//! A fragment without a description is an error instead of being skipped.
//!
//! Supports both single-language and multi-language repository structures:
//! - Single-language: Cargo.toml and changelog.d/ in repository root
//! - Multi-language: Cargo.toml and changelog.d/ in rust/ subfolder
//!
//! Usage: rust-script rust/scripts/collect-changelog.rs [--rust-root <path>]
//!
//! ```cargo
//! [dependencies]
//! regex = "1"
//! chrono = "0.4"
//! ```

use chrono::Utc;
use regex::Regex;
use std::env;
use std::fs;
use std::path::Path;
use std::process::exit;

// Shared with version-and-commit.rs, so both write identical entries.
#[path = "release-changelog.rs"]
mod release_changelog;

fn get_arg(name: &str) -> Option<String> {
    let args: Vec<String> = env::args().collect();
    let flag = format!("--{}", name);

    if let Some(idx) = args.iter().position(|a| a == &flag) {
        return args.get(idx + 1).cloned();
    }

    let env_name = name.to_uppercase().replace('-', "_");
    env::var(&env_name).ok().filter(|s| !s.is_empty())
}

fn get_rust_root() -> String {
    if let Some(root) = get_arg("rust-root") {
        eprintln!("Using explicitly configured Rust root: {}", root);
        return root;
    }

    if Path::new("./Cargo.toml").exists() {
        eprintln!("Detected single-language repository (Cargo.toml in root)");
        return ".".to_string();
    }

    if Path::new("./rust/Cargo.toml").exists() {
        eprintln!("Detected multi-language repository (Cargo.toml in rust/)");
        return "rust".to_string();
    }

    eprintln!("Error: Could not find Cargo.toml in expected locations");
    exit(1);
}

fn get_cargo_toml_path(rust_root: &str) -> String {
    if rust_root == "." {
        "./Cargo.toml".to_string()
    } else {
        format!("{}/Cargo.toml", rust_root)
    }
}

fn get_changelog_dir(rust_root: &str) -> String {
    if rust_root == "." {
        "./changelog.d".to_string()
    } else {
        format!("{}/changelog.d", rust_root)
    }
}

fn get_changelog_path(rust_root: &str) -> String {
    if rust_root == "." {
        "./CHANGELOG.md".to_string()
    } else {
        format!("{}/CHANGELOG.md", rust_root)
    }
}

fn get_version_from_cargo(cargo_toml_path: &str) -> Result<String, String> {
    let content = fs::read_to_string(cargo_toml_path)
        .map_err(|e| format!("Failed to read {}: {}", cargo_toml_path, e))?;

    let re = Regex::new(r#"(?m)^version\s*=\s*"([^"]+)""#).unwrap();

    if let Some(caps) = re.captures(&content) {
        Ok(caps.get(1).unwrap().as_str().to_string())
    } else {
        Err(format!("Could not find version in {}", cargo_toml_path))
    }
}

fn main() {
    let rust_root = get_rust_root();
    let cargo_toml = get_cargo_toml_path(&rust_root);
    let changelog_dir = get_changelog_dir(&rust_root);
    let changelog_file = get_changelog_path(&rust_root);

    let version = match get_version_from_cargo(&cargo_toml) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    };

    println!("Collecting changelog fragments for version {}", version);

    let date_str = Utc::now().format("%Y-%m-%d").to_string();
    match release_changelog::collect_changelog_with_date(
        &changelog_dir,
        &changelog_file,
        &version,
        &date_str,
    ) {
        Ok(0) => {
            println!("No changelog fragments found");
            exit(0);
        }
        Ok(_) => println!("Updated {} with version {}", changelog_file, version),
        Err(e) => {
            eprintln!("Error: {}", e);
            exit(1);
        }
    }

    println!("Changelog collection complete");
}
