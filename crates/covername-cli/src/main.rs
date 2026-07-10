//! Covername CLI — local-first document anonymization.

use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use covername_core::config::Config;
use covername_core::detection::{CustomRuleStore, Detection, RuleEngine};
use covername_core::document::{DocumentType, collect_files, detect_file_type};
use covername_core::export;
use covername_core::ignore::IgnoreList;
use covername_core::mapping::MappingStore;
use covername_core::ner::model_manager::ModelManager;
use covername_core::output;
use covername_core::processor::{self, ResolvedDetection};
use covername_core::replacement;
use covername_core::smart_detection;
use tracing::info;

/// Controls how the CLI renders output.
struct OutputOptions {
    quiet: bool,
    json: bool,
}

/// A local-first document anonymization tool.
///
/// Detects PII in documents and replaces it with consistent
/// cover identities — entirely offline.
#[derive(Parser)]
#[command(name = "covername", version, about)]
struct Cli {
    /// Suppress progress bars and informational output. Only errors and results are printed.
    #[arg(long, short, global = true, default_value_t = false)]
    quiet: bool,

    /// Output results as JSON. Useful for scripting and AI agent integration.
    #[arg(long, global = true, default_value_t = false)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Manage application configuration.
    Config {
        #[command(subcommand)]
        action: ConfigAction,
    },
    /// Manage replacement mappings.
    Mappings {
        #[command(subcommand)]
        action: MappingsAction,
    },
    /// Manage PII detection rules.
    Rules {
        #[command(subcommand)]
        action: RulesAction,
    },
    /// Manage NER model installation and status.
    Model {
        #[command(subcommand)]
        action: ModelAction,
    },
    /// Scan a file or directory for PII without modifying anything.
    #[command(
        long_about = "Scan a file or directory for PII without modifying anything.\n\n\
            Analyzes the input using regex rules and dictionary-based NER to detect \
            names, addresses, SSNs, phone numbers, emails, credit card numbers, and \
            account numbers. Reports findings but does not create any output files.\n\n\
            Use -r/--recursive to scan all supported files in subdirectories."
    )]
    Scan {
        /// Path to the file or directory to scan.
        path: PathBuf,

        /// Recursively scan subdirectories.
        #[arg(long, short, default_value_t = false)]
        recursive: bool,
    },
    /// Process a file or directory: detect PII, review replacements, and write output.
    #[command(
        long_about = "Process a file or directory: detect PII, review replacements, and write output.\n\n\
            Runs the full anonymization pipeline: detect PII, interactively review each \
            detection (accept, edit, or reject), then generate a clean output file with \
            replacements applied. The original file is never modified.\n\n\
            Use -r/--recursive to process all supported files in subdirectories.\n\n\
            Use --auto to skip interactive review and accept all detections automatically. \
            This is useful for scripting, CI pipelines, and AI agent integrations."
    )]
    Process {
        /// Path to the file or directory to process.
        path: PathBuf,

        /// Recursively process subdirectories.
        #[arg(long, short, default_value_t = false)]
        recursive: bool,

        /// Automatically accept all detections without interactive review.
        /// Useful for scripting, CI pipelines, and AI agent use.
        #[arg(long, default_value_t = false)]
        auto_accept: bool,
    },
    /// Manage the ignore list (entities that are always skipped during detection).
    Ignore {
        #[command(subcommand)]
        action: IgnoreAction,
    },
    /// Manage Smart Detection (local AI for improved accuracy).
    #[command(name = "smart-detection")]
    SmartDetection {
        #[command(subcommand)]
        action: SmartDetectionAction,
    },
}

#[derive(Subcommand)]
enum ConfigAction {
    /// Print current configuration as JSON.
    Show,
    /// Update a single configuration field.
    Set {
        /// The configuration key to update.
        key: String,
        /// The new value.
        value: String,
    },
    /// Print the storage directory path.
    Path,
    /// Export configuration, mappings, and rules to a ZIP file.
    Export {
        /// Output path for the ZIP archive.
        #[arg(long)]
        output: PathBuf,
    },
    /// Import configuration, mappings, and rules from a ZIP file.
    Import {
        /// Path to the ZIP archive to import.
        #[arg(long)]
        input: PathBuf,
    },
}

#[derive(Subcommand)]
enum MappingsAction {
    /// List all stored mappings.
    List,
    /// Add a new mapping.
    Add {
        /// The original PII value.
        #[arg(long)]
        original: String,
        /// The replacement value.
        #[arg(long)]
        replacement: String,
        /// The entity type (e.g., PERSON, PHONE, SSN).
        #[arg(long = "type")]
        entity_type: String,
    },
    /// Remove a mapping by its original value.
    Remove {
        /// The original PII value to remove.
        #[arg(long)]
        original: String,
    },
}

#[derive(Subcommand)]
enum RulesAction {
    /// List all rules (built-in and custom).
    List,
    /// Add a custom detection rule.
    Add {
        /// Name for the rule.
        #[arg(long)]
        name: String,
        /// Regex pattern to match.
        #[arg(long)]
        pattern: String,
        /// Entity type for matches (e.g., ACCOUNT_NUMBER).
        #[arg(long = "type")]
        entity_type: String,
    },
    /// Remove a custom rule by name.
    Remove {
        /// Name of the rule to remove.
        #[arg(long)]
        name: String,
    },
    /// Test a pattern against input text.
    Test {
        /// Regex pattern to test.
        #[arg(long)]
        pattern: String,
        /// Text to test against (reads from stdin if not provided).
        #[arg(long)]
        input: Option<String>,
    },
}

#[derive(Subcommand)]
enum ModelAction {
    /// Show NER model installation status.
    Status,
    /// Print the model storage directory path.
    Path,
    /// Download or prepare the ONNX NER model.
    Download,
    /// Remove the ONNX model files (reverts to dictionary detector).
    Remove,
}

#[derive(Subcommand)]
enum IgnoreAction {
    /// List all ignored entities.
    List,
    /// Add an entity to the ignore list.
    Add {
        /// The text to ignore.
        #[arg(long)]
        text: String,
        /// The entity type (e.g., ADDRESS, PERSON). Defaults to UNKNOWN.
        #[arg(long = "type", default_value = "UNKNOWN")]
        entity_type: String,
    },
    /// Remove an entity from the ignore list.
    Remove {
        /// The text to remove from the ignore list.
        #[arg(long)]
        text: String,
    },
    /// Remove all entries from the ignore list.
    Clear,
}

#[derive(Subcommand)]
enum SmartDetectionAction {
    /// Show Smart Detection status.
    Status,
    /// Download the Smart Detection model (~1GB).
    Download,
    /// Remove the Smart Detection model.
    Remove,
}

fn config_path() -> Result<PathBuf> {
    Config::config_path().context("failed to determine storage directory")
}

fn mappings_path() -> Result<PathBuf> {
    Config::mappings_path().context("failed to determine storage directory")
}

fn rules_path() -> Result<PathBuf> {
    Config::rules_path().context("failed to determine storage directory")
}

fn ignore_list_path() -> Result<PathBuf> {
    Config::ignore_list_path().context("failed to determine storage directory")
}

fn handle_config(action: ConfigAction) -> Result<()> {
    let path = config_path()?;

    match action {
        ConfigAction::Show => {
            let config = Config::load(&path).context("failed to load config")?;
            let json = serde_json::to_string_pretty(&config)?;
            println!("{json}");
        }
        ConfigAction::Set { key, value } => {
            let mut config = Config::load(&path).context("failed to load config")?;
            config
                .set(&key, &value)
                .context("failed to set config value")?;
            config.save(&path).context("failed to save config")?;
            println!("Set {key} = {value}");
        }
        ConfigAction::Path => {
            let dir = Config::storage_dir().context("failed to determine storage directory")?;
            println!("{}", dir.display());
        }
        ConfigAction::Export { output } => {
            let storage_dir =
                Config::ensure_storage_dir().context("failed to determine storage directory")?;
            export::export_config(&storage_dir, &output)
                .context("failed to export configuration")?;
            println!("Exported configuration to {}", output.display());
        }
        ConfigAction::Import { input } => {
            let storage_dir =
                Config::ensure_storage_dir().context("failed to determine storage directory")?;
            let result = export::import_config(&input, &storage_dir)
                .context("failed to import configuration")?;

            if result.files_imported.is_empty() {
                println!("No configuration files found in archive.");
            } else {
                println!("Imported {} file(s):", result.files_imported.len());
                for file in &result.files_imported {
                    println!("  - {file}");
                }
            }
        }
    }

    Ok(())
}

fn handle_mappings(action: MappingsAction, opts: &OutputOptions) -> Result<()> {
    let path = mappings_path()?;

    match action {
        MappingsAction::List => {
            let store = MappingStore::load(&path).context("failed to load mappings")?;
            let mappings = store.list();

            if opts.json {
                let json_mappings: Vec<serde_json::Value> = mappings
                    .iter()
                    .map(|m| {
                        serde_json::json!({
                            "original": m.original,
                            "replacement": m.replacement,
                            "entity_type": m.entity_type,
                            "last_used": m.last_used.format("%Y-%m-%dT%H:%M:%S").to_string(),
                        })
                    })
                    .collect();
                println!("{}", serde_json::to_string_pretty(&json_mappings)?);
            } else if mappings.is_empty() {
                if !opts.quiet {
                    println!("No mappings stored.");
                }
            } else {
                println!(
                    "{:<30} {:<30} {:<15} LAST USED",
                    "ORIGINAL", "REPLACEMENT", "TYPE"
                );
                println!("{}", "-".repeat(90));
                for m in mappings {
                    println!(
                        "{:<30} {:<30} {:<15} {}",
                        m.original,
                        m.replacement,
                        m.entity_type,
                        m.last_used.format("%Y-%m-%d %H:%M")
                    );
                }
            }
        }
        MappingsAction::Add {
            original,
            replacement,
            entity_type,
        } => {
            let mut store = MappingStore::load(&path).context("failed to load mappings")?;
            store
                .add(&original, &replacement, &entity_type)
                .context("failed to add mapping")?;
            println!("Added mapping: {original} -> {replacement} [{entity_type}]");
        }
        MappingsAction::Remove { original } => {
            let mut store = MappingStore::load(&path).context("failed to load mappings")?;
            let removed = store
                .remove(&original)
                .context("failed to remove mapping")?;
            if removed {
                println!("Removed mapping for: {original}");
            } else {
                println!("No mapping found for: {original}");
            }
        }
    }

    Ok(())
}

fn handle_rules(action: RulesAction) -> Result<()> {
    let path = rules_path()?;

    match action {
        RulesAction::List => {
            let engine = RuleEngine::new().context("failed to create rule engine")?;
            let custom_store =
                CustomRuleStore::load(&path).context("failed to load custom rules")?;

            // Show built-in rules
            println!("{:<25} {:<18} {:<8} SOURCE", "NAME", "TYPE", "ENABLED");
            println!("{}", "-".repeat(70));
            for rule in engine.rules() {
                if rule.built_in {
                    println!(
                        "{:<25} {:<18} {:<8} built-in",
                        rule.name, rule.entity_type, "yes"
                    );
                }
            }
            // Show custom rules
            for rule in custom_store.list() {
                let enabled = if rule.enabled { "yes" } else { "no" };
                println!(
                    "{:<25} {:<18} {:<8} custom",
                    rule.name, rule.entity_type, enabled
                );
            }

            drop(custom_store);
        }
        RulesAction::Add {
            name,
            pattern,
            entity_type,
        } => {
            let mut store = CustomRuleStore::load(&path).context("failed to load custom rules")?;
            store
                .add(&name, &pattern, &entity_type)
                .context("failed to add rule")?;
            println!("Added rule: {name} ({entity_type})");
        }
        RulesAction::Remove { name } => {
            let mut store = CustomRuleStore::load(&path).context("failed to load custom rules")?;
            let removed = store.remove(&name).context("failed to remove rule")?;
            if removed {
                println!("Removed rule: {name}");
            } else {
                println!("No custom rule found with name: {name}");
            }
        }
        RulesAction::Test { pattern, input } => {
            let text = match input {
                Some(t) => t,
                None => {
                    use std::io::Read;
                    let mut buf = String::new();
                    std::io::stdin()
                        .read_to_string(&mut buf)
                        .context("failed to read stdin")?;
                    buf
                }
            };

            let detections =
                RuleEngine::test_pattern(&pattern, &text).context("failed to test pattern")?;

            if detections.is_empty() {
                println!("No matches found.");
            } else {
                println!("Found {} match(es):\n", detections.len());
                for (i, d) in detections.iter().enumerate() {
                    println!("  {}. \"{}\"", i + 1, d.matched_text);
                    println!("     Position: {}..{}", d.start, d.end);
                    println!("     Context:  {}\n", d.context);
                }
            }
        }
    }

    Ok(())
}

fn handle_ignore(action: IgnoreAction) -> Result<()> {
    let path = ignore_list_path()?;

    match action {
        IgnoreAction::List => {
            let list = IgnoreList::load(&path).context("failed to load ignore list")?;
            let entries = list.list();

            if entries.is_empty() {
                println!("No ignored entities.");
            } else {
                println!("{:<40} {:<15} ADDED", "TEXT", "TYPE");
                println!("{}", "-".repeat(75));
                for e in entries {
                    println!(
                        "{:<40} {:<15} {}",
                        e.text,
                        e.entity_type,
                        e.created.format("%Y-%m-%d %H:%M")
                    );
                }
            }
        }
        IgnoreAction::Add { text, entity_type } => {
            let mut list = IgnoreList::load(&path).context("failed to load ignore list")?;
            list.add(&text, &entity_type)
                .context("failed to add to ignore list")?;
            println!("Added to ignore list: \"{text}\" [{entity_type}]");
        }
        IgnoreAction::Remove { text } => {
            let mut list = IgnoreList::load(&path).context("failed to load ignore list")?;
            let removed = list
                .remove(&text)
                .context("failed to remove from ignore list")?;
            if removed {
                println!("Removed from ignore list: \"{text}\"");
            } else {
                println!("Not found in ignore list: \"{text}\"");
            }
        }
        IgnoreAction::Clear => {
            let list = IgnoreList::load(&path).context("failed to load ignore list")?;
            if list.list().is_empty() {
                println!("Ignore list is already empty.");
                return Ok(());
            }

            print!(
                "Remove all {} ignored entities? (y/n) > ",
                list.list().len()
            );
            std::io::stdout()
                .flush()
                .context("failed to flush stdout")?;

            let stdin = std::io::stdin();
            let mut reader = stdin.lock();
            let mut input = String::new();
            reader
                .read_line(&mut input)
                .context("failed to read input")?;

            if input.trim().to_lowercase() == "y" {
                let mut list = list;
                list.clear().context("failed to clear ignore list")?;
                println!("Ignore list cleared.");
            } else {
                println!("Cancelled.");
            }
        }
    }

    Ok(())
}

/// Extract text from a file using the unified pipeline.
fn extract_text(file: &std::path::Path) -> Result<String> {
    covername_core::pipeline::extract_text(file)
        .context(format!("failed to extract text from {}", file.display()))
}

/// Validate that an input path exists and is readable. Provides clear error messages.
fn validate_input_path(path: &std::path::Path) -> Result<()> {
    if !path.exists() {
        anyhow::bail!("file not found: {}", path.display());
    }
    // Check read permission by attempting metadata access
    match std::fs::metadata(path) {
        Ok(_) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => {
            anyhow::bail!("permission denied: {}", path.display());
        }
        Err(_) => Ok(()), // Other metadata errors will be caught later
    }
}

/// Run detection on text using the unified pipeline.
fn run_detection(text: &str, storage_dir: &std::path::Path) -> Result<Vec<Detection>> {
    covername_core::pipeline::detect_pii(text, storage_dir).context("failed to run PII detection")
}

fn handle_scan(path: PathBuf, recursive: bool, opts: &OutputOptions) -> Result<()> {
    info!(path = %path.display(), recursive, "scanning path");
    validate_input_path(&path)?;

    let files = collect_files(&path, recursive)
        .context(format!("failed to collect files from {}", path.display()))?;

    if files.is_empty() {
        if opts.json {
            println!("[]");
        } else if !opts.quiet {
            println!("No supported files found in {}.", path.display());
        }
        return Ok(());
    }

    let storage_dir =
        Config::ensure_storage_dir().context("failed to determine storage directory")?;
    let is_batch = files.len() > 1;

    // Load ignore list to filter out permanently-ignored entities
    let ignore_path = Config::ignore_list_path().context("failed to determine ignore list path")?;
    let ignore_list = match IgnoreList::load(&ignore_path) {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(path = %ignore_path.display(), error = %e, "failed to load ignore list, using empty");
            IgnoreList::empty()
        }
    };

    // Collect all results for JSON output
    let mut all_results: Vec<serde_json::Value> = Vec::new();

    for file in &files {
        let text = match extract_text(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error: {e:#}");
                continue;
            }
        };

        let detections = run_detection(&text, &storage_dir)?;
        let detections: Vec<_> = detections
            .into_iter()
            .filter(|d| !ignore_list.is_ignored(&d.matched_text))
            .collect();

        if opts.json {
            let file_result = serde_json::json!({
                "file": file.display().to_string(),
                "detections": detections.iter().map(|d| serde_json::json!({
                    "entity_type": d.entity_type,
                    "matched_text": d.matched_text,
                    "rule_name": d.rule_name,
                    "start": d.start,
                    "end": d.end,
                    "context": d.context,
                })).collect::<Vec<_>>()
            });
            all_results.push(file_result);
        } else if !opts.quiet {
            if is_batch {
                println!(
                    "=== {} ({} detection(s)) ===",
                    file.display(),
                    detections.len()
                );
            } else if detections.is_empty() {
                println!("No PII detected in {}.", file.display());
                return Ok(());
            } else {
                println!(
                    "Found {} detection(s) in {}:\n",
                    detections.len(),
                    file.display()
                );
            }

            for (i, d) in detections.iter().enumerate() {
                println!("  {}. [{}] \"{}\"", i + 1, d.entity_type, d.matched_text);
                println!("     Rule: {}", d.rule_name);
                println!("     Position: {}..{}", d.start, d.end);
                println!("     Context: {}\n", d.context);
            }
        }
    }

    if opts.json {
        if is_batch {
            println!("{}", serde_json::to_string_pretty(&all_results)?);
        } else if let Some(result) = all_results.first() {
            println!("{}", serde_json::to_string_pretty(result)?);
        }
    }

    Ok(())
}

/// Holds per-file detection results for batch processing.
struct FileDetections {
    path: PathBuf,
    text: String,
    detections: Vec<Detection>,
}

fn handle_process(
    path: PathBuf,
    recursive: bool,
    auto_accept: bool,
    opts: &OutputOptions,
) -> Result<()> {
    info!(path = %path.display(), recursive, auto_accept, "processing path");
    validate_input_path(&path)?;

    let files = collect_files(&path, recursive)
        .context(format!("failed to collect files from {}", path.display()))?;

    if files.is_empty() {
        println!("No supported files found in {}.", path.display());
        return Ok(());
    }

    let storage_dir =
        Config::ensure_storage_dir().context("failed to determine storage directory")?;

    // Phase 1: Read all files and run detection
    let is_batch = files.len() > 1;
    let mut file_detections: Vec<FileDetections> = Vec::new();
    for (file_idx, file) in files.iter().enumerate() {
        if is_batch && !opts.quiet {
            println!(
                "\n[{}/{}] Scanning {}",
                file_idx + 1,
                files.len(),
                file.file_name()
                    .map(|f| f.to_string_lossy())
                    .unwrap_or_default()
            );
        }

        let text = match extract_text(file) {
            Ok(t) => t,
            Err(e) => {
                eprintln!("Error: {e:#}");
                continue;
            }
        };

        let detections = run_detection(&text, &storage_dir)?;
        file_detections.push(FileDetections {
            path: file.clone(),
            text,
            detections,
        });
    }

    // Phase 2: Resolve replacements with shared mapping store
    let mapping_path = Config::mappings_path().context("failed to determine mappings path")?;
    let mut mapping_store = MappingStore::load(&mapping_path).context("failed to load mappings")?;

    let total_detections: usize = file_detections.iter().map(|f| f.detections.len()).sum();

    if total_detections == 0 {
        if is_batch {
            println!(
                "No PII detected across {} files. No output generated.",
                file_detections.len()
            );
        } else {
            println!(
                "No PII detected in {}. No output generated.",
                file_detections[0].path.display()
            );
        }
        return Ok(());
    }

    // Build resolved detections per file
    let mut resolved_per_file: Vec<(PathBuf, String, Vec<ResolvedDetection>)> = Vec::new();
    for fd in &file_detections {
        let resolved =
            processor::resolve_detections(fd.detections.clone(), &mapping_store, &|orig, etype| {
                replacement::suggest_replacement(orig, etype)
            });
        resolved_per_file.push((fd.path.clone(), fd.text.clone(), resolved));
    }

    // Load ignore list and filter out ignored detections
    let il_path = Config::ignore_list_path().context("failed to determine ignore list path")?;
    let mut ignore_list = match IgnoreList::load(&il_path) {
        Ok(list) => list,
        Err(e) => {
            tracing::warn!(path = %il_path.display(), error = %e, "failed to load ignore list, using empty");
            IgnoreList::empty()
        }
    };
    for (_, _, resolved) in &mut resolved_per_file {
        resolved.retain(|r| !ignore_list.is_ignored(&r.detection.matched_text));
    }

    // Recalculate total after filtering
    let total_after_ignore: usize = resolved_per_file.iter().map(|(_, _, r)| r.len()).sum();

    if total_after_ignore == 0 {
        if is_batch {
            println!(
                "All detections are in the ignore list. No output generated ({} file(s) scanned).",
                file_detections.len()
            );
        } else {
            println!(
                "All detections are in the ignore list. No output generated for {}.",
                file_detections[0].path.display()
            );
        }
        return Ok(());
    }

    // Phase 3: Interactive review (or auto-accept)
    if auto_accept {
        // Auto mode: accept all detections, save mappings, skip interactive review
        for (_, _, resolved) in &mut resolved_per_file {
            for item in resolved.iter_mut() {
                item.accepted = true;
                mapping_store
                    .add(
                        &item.detection.matched_text,
                        &item.replacement,
                        &item.detection.entity_type,
                    )
                    .context("failed to save mapping")?;
            }
        }
        if is_batch {
            println!(
                "Auto-accepted {} detection(s) across {} file(s).\n",
                total_after_ignore,
                resolved_per_file.len()
            );
        } else {
            println!("Auto-accepted {} detection(s).\n", total_after_ignore);
        }
    } else {
        // Interactive review
        if is_batch {
            println!(
                "Found {} detection(s) across {} file(s). Starting interactive review...\n",
                total_after_ignore,
                resolved_per_file.len()
            );
        } else {
            println!(
                "Found {} detection(s). Starting interactive review...\n",
                total_after_ignore
            );
        }

        let stdin = std::io::stdin();
        let mut reader = stdin.lock();
        let mut quit_review = false;
        let mut accept_all = false;

        for (file_path, _, resolved) in &mut resolved_per_file {
            if quit_review {
                break;
            }

            // Re-filter against ignore list (catches entries added during earlier files)
            resolved.retain(|r| !ignore_list.is_ignored(&r.detection.matched_text));

            if resolved.is_empty() {
                continue;
            }

            if is_batch {
                println!(
                    "=== {} ({} detection(s)) ===\n",
                    file_path.display(),
                    resolved.len()
                );
            }

            if accept_all {
                for item in resolved.iter_mut() {
                    item.accepted = true;
                    mapping_store
                        .add(
                            &item.detection.matched_text,
                            &item.replacement,
                            &item.detection.entity_type,
                        )
                        .context("failed to save mapping")?;
                }
                continue;
            }

            let mut i = 0;
            while i < resolved.len() {
                let d = &resolved[i];
                println!(
                    "[{}/{}] {}: \"{}\"",
                    i + 1,
                    resolved.len(),
                    d.detection.entity_type,
                    d.detection.matched_text
                );
                println!("  Context: {}", d.detection.context);
                println!("  Replace with: \"{}\"", d.replacement);
                print!("  (y) accept  (e) edit  (n) reject  (a) accept all  (q) quit > ");
                std::io::stdout()
                    .flush()
                    .context("failed to flush stdout")?;

                let mut input = String::new();
                reader
                    .read_line(&mut input)
                    .context("failed to read input")?;
                let choice = input.trim().to_lowercase();

                match choice.as_str() {
                    "y" | "" => {
                        resolved[i].accepted = true;
                        mapping_store
                            .add(
                                &resolved[i].detection.matched_text,
                                &resolved[i].replacement,
                                &resolved[i].detection.entity_type,
                            )
                            .context("failed to save mapping")?;
                        println!("  ✓ Accepted\n");
                        i += 1;
                    }
                    "e" => {
                        print!("  Enter replacement: ");
                        std::io::stdout()
                            .flush()
                            .context("failed to flush stdout")?;
                        let mut new_replacement = String::new();
                        reader
                            .read_line(&mut new_replacement)
                            .context("failed to read input")?;
                        let new_replacement = new_replacement.trim().to_string();

                        if new_replacement.is_empty() {
                            println!("  (empty input — keeping original suggestion)\n");
                        } else {
                            resolved[i].replacement = new_replacement;
                        }
                        resolved[i].accepted = true;
                        mapping_store
                            .add(
                                &resolved[i].detection.matched_text,
                                &resolved[i].replacement,
                                &resolved[i].detection.entity_type,
                            )
                            .context("failed to save mapping")?;
                        println!("  ✓ Accepted with edit\n");
                        i += 1;
                    }
                    "n" => {
                        resolved[i].accepted = false;
                        println!("  ✗ Rejected");
                        print!("  Always ignore this? (y/n) > ");
                        std::io::stdout()
                            .flush()
                            .context("failed to flush stdout")?;
                        let mut ignore_input = String::new();
                        reader
                            .read_line(&mut ignore_input)
                            .context("failed to read input")?;
                        if ignore_input.trim().to_lowercase() == "y" {
                            ignore_list
                                .add(
                                    &resolved[i].detection.matched_text,
                                    &resolved[i].detection.entity_type,
                                )
                                .context("failed to add to ignore list")?;
                            println!(
                                "  Added to ignore list: \"{}\"\n",
                                resolved[i].detection.matched_text
                            );
                        } else {
                            println!();
                        }
                        i += 1;
                    }
                    "a" => {
                        // Accept all remaining detections across ALL files
                        for item in resolved.iter_mut().skip(i) {
                            item.accepted = true;
                            mapping_store
                                .add(
                                    &item.detection.matched_text,
                                    &item.replacement,
                                    &item.detection.entity_type,
                                )
                                .context("failed to save mapping")?;
                        }
                        println!("  ✓ Accepted all remaining\n");
                        accept_all = true;
                        break;
                    }
                    "q" => {
                        println!("  Quitting review (progress saved).\n");
                        quit_review = true;
                        break;
                    }
                    _ => {
                        println!("  Unknown option. Please enter y, e, n, a, or q.\n");
                    }
                }
            }
        }
    } // end of else (interactive review)

    // Phase 4: Generate output
    let cfg_path = Config::config_path().context("failed to determine config path")?;
    let config = Config::load(&cfg_path).context("failed to load config")?;

    let mut total_accepted = 0;
    let mut output_files: Vec<PathBuf> = Vec::new();

    for (file_path, text, resolved) in &resolved_per_file {
        let accepted_count = resolved.iter().filter(|r| r.accepted).count();
        total_accepted += accepted_count;

        if accepted_count == 0 {
            continue;
        }

        let output_path = output::resolve_output_path(file_path, &config);

        // Choose output method based on file type
        let file_type = detect_file_type(file_path);
        if file_type == DocumentType::Xlsx {
            let replacements: Vec<(String, String)> = resolved
                .iter()
                .filter(|r| r.accepted)
                .map(|r| (r.detection.matched_text.clone(), r.replacement.clone()))
                .collect();

            covername_core::xlsx::write_xlsx(file_path, &replacements, &output_path).context(
                format!("failed to write XLSX output: {}", output_path.display()),
            )?;
        } else if file_type == DocumentType::Pdf && covername_core::ocr::is_ocr_pipeline_available()
        {
            // Use position-aware redaction for PDFs (preserves visual layout)
            let replacements: Vec<(String, String)> = resolved
                .iter()
                .filter(|r| r.accepted)
                .map(|r| (r.detection.matched_text.clone(), r.replacement.clone()))
                .collect();

            covername_core::pipeline::write_redacted_pdf(file_path, &replacements, &output_path)
                .context(format!(
                    "failed to write redacted PDF: {}",
                    output_path.display()
                ))?;
        } else {
            let output_text = processor::apply_replacements(text, resolved);
            output::write_output(&output_text, file_path, &output_path).context(format!(
                "failed to write output file: {}",
                output_path.display()
            ))?;
        }

        output_files.push(output_path.clone());
        info!(output = %output_path.display(), "output file written");
    }

    // Phase 5: Print summary
    if opts.json {
        let result = serde_json::json!({
            "files_processed": file_detections.len(),
            "total_detections": total_detections,
            "accepted": total_accepted,
            "output_files": output_files.iter().map(|f| f.display().to_string()).collect::<Vec<_>>(),
        });
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else if !opts.quiet {
        if is_batch {
            println!("Batch Summary:");
            println!("  Files processed: {}", file_detections.len());
            println!("  Total detections: {total_detections}");
            println!("  Accepted: {total_accepted}");
            if output_files.is_empty() {
                println!("  No output files generated.");
            } else {
                println!("  Output files:");
                for f in &output_files {
                    println!("    - {}", f.display());
                }
            }
        } else if output_files.is_empty() {
            println!("No replacements accepted. No output file written.");
        } else {
            println!("Summary:");
            println!("  Detections: {total_detections}");
            println!("  Accepted:   {total_accepted}");
            println!("  Output:     {}", output_files[0].display());
        }
    }

    Ok(())
}

fn handle_model(action: ModelAction) -> Result<()> {
    let dir = Config::storage_dir().context("failed to determine storage directory")?;
    let manager = ModelManager::new(&dir);

    match action {
        ModelAction::Status => {
            let status = manager.status();
            match status {
                covername_core::ner::ModelStatus::OnnxInstalled { version, path } => {
                    println!("NER Model Status: ONNX model installed");
                    println!("  Version: {version}");
                    println!("  Path:    {}", path.display());
                    println!();
                    #[cfg(feature = "onnx")]
                    println!("  Active detector: ONNX (PII DistilBERT)");
                    #[cfg(not(feature = "onnx"))]
                    {
                        println!("  Active detector: Dictionary (ONNX feature not enabled)");
                        println!("  To use ONNX: rebuild with `cargo build --features onnx`");
                    }
                }
                covername_core::ner::ModelStatus::Installed { version, path } => {
                    println!("NER Model Status: Dictionary detector active");
                    println!("  Version: {version}");
                    println!("  Path:    {}", path.display());
                    println!();
                    println!("  The dictionary detector requires no download.");
                    println!(
                        "  For improved detection, install the ONNX model with `covername model download`."
                    );
                }
                covername_core::ner::ModelStatus::DictionaryOnly => {
                    println!("NER Model Status: Dictionary only");
                    println!("  Run `covername model download` to set up the ONNX model.");
                }
                covername_core::ner::ModelStatus::NotInstalled => {
                    println!("NER Model Status: Not installed");
                    println!("  Run `covername model download` to install.");
                }
                covername_core::ner::ModelStatus::UpdateAvailable { current, latest } => {
                    println!("NER Model Status: Update available");
                    println!("  Current: {current}");
                    println!("  Latest:  {latest}");
                }
            }
        }
        ModelAction::Path => {
            let model_dir = manager.model_dir();
            println!("{}", model_dir.display());
        }
        ModelAction::Download => {
            eprintln!("Downloading PII detection model (barflyman/bert-pii-detect-onnx)...");
            manager
                .download_model()
                .context("failed to download model")?;
        }
        ModelAction::Remove => {
            if manager.is_onnx_installed() {
                manager
                    .remove_model()
                    .context("failed to remove model files")?;
                println!("ONNX model files removed. Reverting to dictionary detector.");
            } else {
                println!("No ONNX model is installed. Nothing to remove.");
            }
        }
    }

    Ok(())
}

fn handle_smart_detection(action: SmartDetectionAction) -> Result<()> {
    match action {
        SmartDetectionAction::Status => {
            let status = smart_detection::status();
            match status {
                smart_detection::SmartDetectionStatus::Installed { model_size_mb } => {
                    println!("Smart Detection: Installed ✓");
                    println!("  Model size: {model_size_mb} MB");
                    if let Ok(dir) = smart_detection::model_dir() {
                        println!("  Path:       {}", dir.display());
                    }
                    println!();
                    #[cfg(feature = "smart-detection")]
                    println!("  Status: Active (will classify detections automatically)");
                    #[cfg(not(feature = "smart-detection"))]
                    {
                        println!("  Status: Model downloaded but feature not compiled");
                        println!(
                            "  To activate: rebuild with `cargo build --features smart-detection`"
                        );
                    }
                }
                smart_detection::SmartDetectionStatus::NotInstalled => {
                    println!("Smart Detection: Not installed");
                    println!();
                    println!("  Smart Detection uses a local AI model to reduce false positives");
                    println!("  by classifying detections as personal vs public/corporate.");
                    println!();
                    println!("  Install with: covername smart-detection download");
                    println!("  Download size: ~1 GB (Qwen2.5-1.5B-Instruct)");
                }
                smart_detection::SmartDetectionStatus::FeatureNotCompiled => {
                    println!("Smart Detection: Model downloaded but feature not compiled");
                    if let Ok(dir) = smart_detection::model_dir() {
                        println!("  Path: {}", dir.display());
                    }
                    println!();
                    println!(
                        "  To activate: rebuild with `cargo build --features smart-detection`"
                    );
                }
            }
        }
        SmartDetectionAction::Download => {
            smart_detection::download_model()
                .context("failed to download Smart Detection model")?;
        }
        SmartDetectionAction::Remove => {
            if smart_detection::is_installed() {
                smart_detection::remove_model()
                    .context("failed to remove Smart Detection model")?;
                println!("Smart Detection model removed.");
            } else {
                println!("Smart Detection model is not installed. Nothing to remove.");
            }
        }
    }

    Ok(())
}

fn setup_logging() {
    use tracing_appender::rolling;
    use tracing_subscriber::{EnvFilter, fmt, layer::SubscriberExt, util::SubscriberInitExt};

    // Log file: ~/.config/covername/logs/covername.log (daily rotation)
    let log_dir = dirs::home_dir()
        .unwrap_or_default()
        .join(".config/covername/logs");
    let file_appender = rolling::daily(&log_dir, "covername.log");

    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        if cfg!(debug_assertions) {
            // In dev mode: capture all our crate logs at debug level
            EnvFilter::new("debug")
        } else {
            EnvFilter::new("info")
        }
    });

    // Log to file directly (blocking) — ensures logs are written immediately.
    // File-only to avoid interference with interactive CLI and progress bars.
    tracing_subscriber::registry()
        .with(env_filter)
        .with(fmt::layer().with_writer(file_appender).with_ansi(false))
        .init();
}

fn main() -> Result<()> {
    setup_logging();
    info!("covername starting");

    let cli = Cli::parse();
    let output_opts = OutputOptions {
        quiet: cli.quiet,
        json: cli.json,
    };

    match cli.command {
        Commands::Config { action } => handle_config(action),
        Commands::Mappings { action } => handle_mappings(action, &output_opts),
        Commands::Rules { action } => handle_rules(action),
        Commands::Model { action } => handle_model(action),
        Commands::Ignore { action } => handle_ignore(action),
        Commands::SmartDetection { action } => handle_smart_detection(action),
        Commands::Scan { path, recursive } => handle_scan(path, recursive, &output_opts),
        Commands::Process {
            path,
            recursive,
            auto_accept,
        } => handle_process(path, recursive, auto_accept, &output_opts),
    }
}
