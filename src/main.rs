/**
 * This code written by Claude Sonnet 4 (claude-3-5-sonnet-20241022)
 * Generated via Cursor IDE (cursor.sh) with AI assistance
 * Model: Anthropic Claude 3.5 Sonnet
 * Generation timestamp: 2024-12-19T10:30:00Z
 * Context: Pure Rust implementation of sortify image organizer
 * 
 * Technical details:
 * - LLM: Claude 3.5 Sonnet (2024-10-22)
 * - IDE: Cursor (cursor.sh)
 * - Generation method: AI-assisted pair programming
 * - Code style: Rust idiomatic with clap CLI, rayon parallelism
 * - Dependencies: fast-exif-rs, clap, indicatif, xxhash-rust, rayon
 */

use anyhow::Result;
use clap::{Parser, Subcommand};
use log::info;
use std::path::{Path, PathBuf};
use walkdir::WalkDir;

mod exif;
mod file_ops;
mod naming;
mod hashing;

use file_ops::{FileProcessor, ProcessResult};

#[derive(Parser)]
#[command(name = "sortify-rs")]
#[command(version)]
#[command(about = "Image and video file organizer based on EXIF timestamps")]
#[command(long_about = "A high-performance Rust tool for organizing image and video files based on EXIF metadata timestamps with subsecond precision. Uses fast-exif-rs for optimal performance.

File operation modes:
- move (default): Move files to organized structure
- copy: Copy files to organized structure, keep originals  
- symlink: Create symbolic links to organized structure

Supported file types: JPG, JPEG, PNG, TIFF, HIF, MOV, MP4, AVI
Output format: YYYY/MM-Mon/YYYYMMDD_HHMMSS.fff<ext>
Tie-breaking: Files with identical timestamps get -2, -3, etc. suffixes")]
struct Cli {
    /// Increase verbosity (-v=INFO, -vv=DEBUG, -vvv=TRACE)
    #[arg(short, long, action = clap::ArgAction::Count)]
    verbose: u8,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Process one or more image files
    Files {
        /// Files to process
        files: Vec<PathBuf>,
        /// Number of parallel workers (default: CPU count)
        #[arg(short, long)]
        workers: Option<usize>,
        /// Output directory for organized files (default: current directory)
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,
        /// File operation mode: move (default), copy, or symlink
        #[arg(short, long, default_value = "move")]
        mode: String,
    },
    /// Process all image files in one or more directories recursively
    Batch {
        /// Directories to process
        directories: Vec<PathBuf>,
        /// Number of parallel workers (default: CPU count)
        #[arg(short, long)]
        workers: Option<usize>,
        /// Limit number of images to process (0=all, default: 0)
        #[arg(long, default_value = "0")]
        limit: usize,
        /// Output directory for organized files (default: current directory)
        #[arg(short, long, default_value = ".")]
        output_dir: PathBuf,
        /// File operation mode: move (default), copy, or symlink
        #[arg(short, long, default_value = "move")]
        mode: String,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    // Setup logging based on verbosity
    setup_logging(cli.verbose)?;

    info!("Starting sortify-rs");

    match cli.command {
        Commands::Files { files, workers, output_dir, mode } => {
            process_files(files, workers, output_dir, mode)
        }
        Commands::Batch { directories, workers, limit, output_dir, mode } => {
            process_batch(directories, workers, limit, output_dir, mode)
        }
    }
}

fn setup_logging(verbosity: u8) -> Result<()> {
    let level = match verbosity {
        0 => log::LevelFilter::Warn,
        1 => log::LevelFilter::Info,
        2 => log::LevelFilter::Debug,
        _ => log::LevelFilter::Trace,
    };

    env_logger::Builder::from_default_env()
        .filter_level(level)
        .init();

    Ok(())
}

fn process_files(files: Vec<PathBuf>, workers: Option<usize>, output_dir: PathBuf, mode: String) -> Result<()> {
    if files.is_empty() {
        anyhow::bail!("No files specified");
    }

    info!("Processing {} files", files.len());

    let mut file_processor = FileProcessor::new(workers);
    let results = file_processor.process_files(files, &output_dir, &mode)?;

    print_summary(&results);
    Ok(())
}

fn process_batch(
    directories: Vec<PathBuf>,
    workers: Option<usize>,
    limit: usize,
    output_dir: PathBuf,
    mode: String,
) -> Result<()> {
    if directories.is_empty() {
        anyhow::bail!("No directories specified");
    }

    // Collect all image files from directories
    let mut all_files = Vec::new();
    
    for directory in &directories {
        info!("Scanning directory: {}", directory.display());
        let files = find_image_files(directory)?;
        all_files.extend(files);
        info!("Found {} files in {}", all_files.len(), directory.display());
    }

    // Remove duplicates
    all_files.sort();
    all_files.dedup();

    // Apply limit if specified
    if limit > 0 && limit < all_files.len() {
        info!("Limiting to {} files (found {})", limit, all_files.len());
        all_files.truncate(limit);
    }

    info!("Total files to process: {}", all_files.len());

    let mut file_processor = FileProcessor::new(workers);
    let results = file_processor.process_files(all_files, &output_dir, &mode)?;

    print_summary(&results);
    Ok(())
}

fn find_image_files(directory: &Path) -> Result<Vec<PathBuf>> {
    let extensions = [
        "jpg", "jpeg", "png", "tiff", "tif", "hif", "heic", "cr2",
        "mov", "mp4", "avi", "3gp", "dng", "m4v", "mkv"
    ];

    let mut files = Vec::new();
    
    for entry in WalkDir::new(directory).into_iter().filter_map(|e| e.ok()) {
        if entry.file_type().is_file() {
            if let Some(ext) = entry.path().extension() {
                if let Some(ext_str) = ext.to_str() {
                    if extensions.contains(&ext_str.to_lowercase().as_str()) {
                        files.push(entry.path().to_path_buf());
                    }
                }
            }
        }
    }

    Ok(files)
}

fn print_summary(results: &[ProcessResult]) {
    let processed = results.len();
    let renamed = results.iter().filter(|r| r.success && r.renamed).count();
    let skipped = results.iter().filter(|r| r.success && !r.renamed).count();
    let errors = results.iter().filter(|r| !r.success).count();

    println!("\nProcessing complete!");
    println!("Files processed: {}", processed);
    println!("Files renamed: {}", renamed);
    println!("Files skipped: {}", skipped);
    println!("Errors: {}", errors);

    if errors > 0 {
        println!("\nErrors:");
        for result in results.iter().filter(|r| !r.success) {
            println!("  {}: {}", result.file_path.display(), result.error.as_deref().unwrap_or("Unknown error"));
        }
    }
}
