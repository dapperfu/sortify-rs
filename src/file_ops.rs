/**
 * File operations module for processing and renaming files
 */

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{debug, info, warn};
use rayon::prelude::*;
use rayon::ThreadPoolBuilder;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::exif::{ExifData, ExifProcessor};
use crate::hashing::ContentHasher;
use crate::naming::FilenameGenerator;

/// Perform file operation based on mode
fn perform_file_operation(source_path: &Path, target_path: &Path, mode: &str) -> Result<()> {
    debug!("Attempting {} operation: '{}' -> '{}'", mode, source_path.display(), target_path.display());
    
    // Check if source file exists
    if !source_path.exists() {
        anyhow::bail!("Source file does not exist: {}", source_path.display());
    }
    
    // Check if target directory exists
    if let Some(parent) = target_path.parent() {
        if !parent.exists() {
            debug!("Creating target directory: {}", parent.display());
            fs::create_dir_all(parent)
                .with_context(|| format!("Failed to create target directory: {}", parent.display()))?;
        }
    }
    
    match mode {
        "move" => {
            debug!("Performing move operation");
            match fs::rename(source_path, target_path) {
                Ok(_) => {
                    debug!("Move operation successful");
                }
                Err(e) if e.kind() == std::io::ErrorKind::CrossesDevices => {
                    debug!("Cross-device move detected, using copy+delete strategy");
                    // Copy the file first
                    fs::copy(source_path, target_path)
                        .with_context(|| format!("Failed to copy file from '{}' to '{}'", 
                            source_path.display(), target_path.display()))?;
                    // Then delete the original
                    fs::remove_file(source_path)
                        .with_context(|| format!("Failed to remove original file: {}", source_path.display()))?;
                    debug!("Cross-device move operation successful");
                }
                Err(e) => {
                    return Err(e).with_context(|| format!("Failed to move file from '{}' to '{}'", 
                        source_path.display(), target_path.display()));
                }
            }
        }
        "copy" => {
            debug!("Performing copy operation");
            fs::copy(source_path, target_path)
                .with_context(|| format!("Failed to copy file from '{}' to '{}'", 
                    source_path.display(), target_path.display()))?;
            debug!("Copy operation successful");
        }
        "symlink" => {
            debug!("Performing symlink operation");
            std::os::unix::fs::symlink(source_path, target_path)
                .with_context(|| format!("Failed to create symlink from '{}' to '{}'", 
                    source_path.display(), target_path.display()))?;
            debug!("Symlink operation successful");
        }
        _ => anyhow::bail!("Invalid mode: {}. Must be 'move', 'copy', or 'symlink'", mode),
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct ProcessResult {
    pub file_path: PathBuf,
    pub success: bool,
    pub renamed: bool,
    pub new_path: Option<PathBuf>,
    pub error: Option<String>,
}

pub struct FileProcessor {
    workers: Option<usize>,
    exif_processor: ExifProcessor,
    filename_generator: FilenameGenerator,
    content_hasher: ContentHasher,
}

impl FileProcessor {
    pub fn new(workers: Option<usize>) -> Self {
        // Configure rayon thread pool for optimal I/O bound performance
        if let Some(worker_count) = workers {
            // For I/O bound workloads, use fewer threads than CPU cores to reduce contention
            let optimal_threads = std::cmp::min(worker_count, num_cpus::get() / 2).max(1);
            
            info!("Configuring rayon thread pool with {} threads (requested: {}, CPUs: {})", 
                  optimal_threads, worker_count, num_cpus::get());
            
            ThreadPoolBuilder::new()
                .num_threads(optimal_threads)
                .thread_name(|i| format!("sortify-worker-{}", i))
                .build_global()
                .unwrap_or_else(|_| {
                    warn!("Failed to configure rayon thread pool, using default");
                });
        } else {
            // Default: use half of CPU cores for I/O bound work
            let default_threads = (num_cpus::get() / 2).max(1);
            info!("Using default thread pool with {} threads (CPUs: {})", 
                  default_threads, num_cpus::get());
            
            ThreadPoolBuilder::new()
                .num_threads(default_threads)
                .thread_name(|i| format!("sortify-worker-{}", i))
                .build_global()
                .unwrap_or_else(|_| {
                    warn!("Failed to configure rayon thread pool, using default");
                });
        }

        Self {
            workers,
            exif_processor: ExifProcessor::new(),
            filename_generator: FilenameGenerator::new(),
            content_hasher: ContentHasher::new(),
        }
    }

    /// Process multiple files with parallel processing and progress tracking
    pub fn process_files(&mut self, files: Vec<PathBuf>, output_dir: &Path, mode: &str) -> Result<Vec<ProcessResult>> {
        info!("Processing {} files", files.len());

        // Convert output directory to absolute path to avoid issues with relative paths
        let output_dir = output_dir.canonicalize()
            .or_else(|_| {
                // If canonicalize fails (e.g., directory doesn't exist), create it and try again
                fs::create_dir_all(output_dir)
                    .context("Failed to create output directory")?;
                output_dir.canonicalize()
                    .context("Failed to canonicalize output directory")
            })?;

        // First pass: Extract EXIF data and generate filenames in parallel
        let analysis_results = self.analyze_files_parallel(files.clone())?;

        // Build content hash index for duplicate detection
        let hash_index = self.build_content_hash_index(&analysis_results, &output_dir)?;

        // Second pass: Handle file operations with parallel directory processing
        let results = self.rename_files_parallel(analysis_results, &hash_index, &output_dir, mode)?;

        Ok(results)
    }

    fn analyze_files_parallel(&mut self, files: Vec<PathBuf>) -> Result<Vec<AnalysisResult>> {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec:.1} files/s) ETA: {eta} {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("Analyzing files");

        // Use parallel processing with rayon for maximum performance
        // Process files in chunks to reduce memory pressure and improve cache locality
        let chunk_size = std::cmp::max(100, files.len() / rayon::current_num_threads());
        let pb = Arc::new(pb);
        
        // Create a new ExifProcessor for each thread to avoid borrowing issues
        let results: Vec<AnalysisResult> = files
            .par_chunks(chunk_size)
            .flat_map(|chunk| {
                chunk.par_iter().map(|file_path| {
                    // Create a new processor for each thread
                    let mut temp_processor = crate::exif::ExifProcessor::new();
                    let result = temp_processor.analyze_single_file(file_path);
                    pb.inc(1);
                    result
                }).collect::<Vec<_>>()
            })
            .collect();

        pb.finish_with_message("Analysis complete");
        Ok(results)
    }

    fn analyze_single_file(&mut self, file_path: &Path) -> AnalysisResult {
        // Skip symlinks
        if file_path.is_symlink() {
            return AnalysisResult {
                file_path: file_path.to_path_buf(),
                success: false,
                error: Some("Skipped symlink".to_string()),
                exif_data: None,
                new_filename: None,
            };
        }

        match self.exif_processor.extract_exif_data(file_path) {
            Ok(exif_data) => {
                let extension = self.get_file_extension(file_path);
                debug!("Generated extension: '{}' for file: {}", extension, file_path.display());
                debug!("EXIF timestamp: {} ({}ms)", exif_data.timestamp, exif_data.milliseconds);
                
                let new_filename = self.filename_generator.generate_filename(
                    exif_data.timestamp,
                    exif_data.milliseconds,
                    &extension,
                    &[], // Will be updated with existing files later
                );
                
                debug!("Generated filename: '{}'", new_filename);

                AnalysisResult {
                    file_path: file_path.to_path_buf(),
                    success: true,
                    error: None,
                    exif_data: Some(exif_data),
                    new_filename: Some(new_filename),
                }
            }
            Err(e) => {
                AnalysisResult {
                    file_path: file_path.to_path_buf(),
                    success: false,
                    error: Some(e.to_string()),
                    exif_data: None,
                    new_filename: None,
                }
            }
        }
    }

    fn build_content_hash_index(
        &self,
        analysis_results: &[AnalysisResult],
        output_dir: &Path,
    ) -> Result<HashMap<PathBuf, String>> {
        // Only hash files that would actually conflict
        let mut files_to_hash = Vec::new();
        let mut target_paths = HashMap::new();

        for result in analysis_results {
            if result.success {
                if let (Some(_exif_data), Some(new_filename)) = (&result.exif_data, &result.new_filename) {
                    let target_path = output_dir.join(new_filename);
                    target_paths.insert(result.file_path.clone(), target_path.clone());
                    
                    // Check if target path already exists
                    if target_path.exists() {
                        files_to_hash.push(result.file_path.clone());
                        files_to_hash.push(target_path);
                    }
                }
            }
        }

        if files_to_hash.is_empty() {
            info!("No file conflicts detected, skipping hash index building");
            return Ok(HashMap::new());
        }

        info!("Building hash index for {} potentially conflicting files", files_to_hash.len());
        
        let pb = ProgressBar::new(files_to_hash.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec:.1} files/s) ETA: {eta} {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("Building hash index");

        // Use parallel processing for hash calculation
        let hash_results = Arc::new(Mutex::new(HashMap::new()));
        let pb = Arc::new(pb);

        files_to_hash.par_iter().for_each(|file_path| {
            match self.content_hasher.calculate_file_hash(file_path) {
                Ok(hash) => {
                    let mut hash_results = hash_results.lock().unwrap();
                    hash_results.insert(file_path.clone(), hash);
                }
                Err(e) => {
                    warn!("Failed to calculate hash for {}: {}", file_path.display(), e);
                }
            }
            pb.inc(1);
        });

        pb.finish_with_message("Hash index complete");
        
        let hash_index = Arc::try_unwrap(hash_results).unwrap().into_inner().unwrap();
        Ok(hash_index)
    }

    fn rename_files_parallel(
        &self,
        analysis_results: Vec<AnalysisResult>,
        hash_index: &HashMap<PathBuf, String>,
        output_dir: &Path,
        mode: &str,
    ) -> Result<Vec<ProcessResult>> {
        let pb = ProgressBar::new(analysis_results.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec:.1} files/s) ETA: {eta} {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("Renaming files");

        // Group files by target directory to minimize conflicts
        let mut grouped_results = HashMap::new();
        for result in analysis_results {
            if let Some(exif_data) = &result.exif_data {
                let target_dir = output_dir.join(format!("{}/{}", 
                    exif_data.timestamp.format("%Y"), 
                    exif_data.timestamp.format("%m-%b")));
                grouped_results.entry(target_dir).or_insert_with(Vec::new).push(result);
            }
        }

        // Process each directory group in parallel
        let pb = Arc::new(pb);
        let hash_index = Arc::new(hash_index);
        let output_dir = Arc::new(output_dir.to_path_buf());
        
        let mut all_results = Vec::new();
        
        // Process directory groups in parallel
        let group_results: Vec<Vec<ProcessResult>> = grouped_results
            .into_par_iter()
            .map(|(_target_dir, results)| {
                let mut group_results = Vec::new();
                let mut existing_files = Vec::new();
                
                // Within each group, process files sequentially to avoid conflicts
                for result in results {
                    let process_result = self.process_single_file_rename(
                        result,
                        &hash_index,
                        &output_dir,
                        &mut existing_files,
                        mode,
                    );
                    group_results.push(process_result);
                }
                group_results
            })
            .collect();

        // Flatten results and update progress
        for group_result in group_results {
            for process_result in group_result {
                match &process_result {
                    ProcessResult { renamed: true, .. } => {
                        let msg = format!("Renamed: {}", process_result.file_path.display());
                        pb.set_message(msg);
                    }
                    ProcessResult { success: false, .. } => {
                        let msg = format!("Error: {}", process_result.file_path.display());
                        pb.set_message(msg);
                    }
                    _ => {
                        let msg = format!("Skipped: {}", process_result.file_path.display());
                        pb.set_message(msg);
                    }
                }
                all_results.push(process_result);
                pb.inc(1);
            }
        }

        pb.finish_with_message("Renaming complete");
        Ok(all_results)
    }

    fn process_single_file_rename(
        &self,
        analysis_result: AnalysisResult,
        hash_index: &HashMap<PathBuf, String>,
        output_dir: &Path,
        existing_files: &mut Vec<String>,
        mode: &str,
    ) -> ProcessResult {
        if !analysis_result.success {
            return ProcessResult {
                file_path: analysis_result.file_path,
                success: false,
                renamed: false,
                new_path: None,
                error: analysis_result.error,
            };
        }

        let (exif_data, _new_filename) = match (analysis_result.exif_data, analysis_result.new_filename) {
            (Some(exif_data), Some(filename)) => (exif_data, filename),
            _ => {
                return ProcessResult {
                    file_path: analysis_result.file_path,
                    success: false,
                    renamed: false,
                    new_path: None,
                    error: Some("Missing EXIF data or filename".to_string()),
                };
            }
        };

        // Generate final filename with tie-breaking
        let final_filename = self.filename_generator.generate_filename(
            exif_data.timestamp,
            exif_data.milliseconds,
            &self.get_file_extension(&analysis_result.file_path),
            existing_files,
        );

        let target_path = output_dir.join(&final_filename);

        // Check for content duplicates
        if target_path.exists() {
            if let (Some(input_hash), Some(existing_hash)) = (
                hash_index.get(&analysis_result.file_path),
                hash_index.get(&target_path),
            ) {
                if input_hash == existing_hash {
                    return ProcessResult {
                        file_path: analysis_result.file_path,
                        success: true,
                        renamed: false,
                        new_path: None,
                        error: Some("Content duplicate".to_string()),
                    };
                }
            }
        }

        // Create directory structure
        if let Some(parent) = target_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                return ProcessResult {
                    file_path: analysis_result.file_path,
                    success: false,
                    renamed: false,
                    new_path: None,
                    error: Some(format!("Failed to create directory: {}", e)),
                };
            }
        }

        // Check if file would be renamed to itself
        if target_path == analysis_result.file_path {
            return ProcessResult {
                file_path: analysis_result.file_path,
                success: true,
                renamed: false,
                new_path: None,
                error: Some("No rename needed".to_string()),
            };
        }

        // Perform file operation based on mode
        match perform_file_operation(&analysis_result.file_path, &target_path, mode) {
            Ok(_) => {
                existing_files.push(final_filename);
                ProcessResult {
                    file_path: analysis_result.file_path,
                    success: true,
                    renamed: true,
                    new_path: Some(target_path),
                    error: None,
                }
            }
            Err(e) => {
                ProcessResult {
                    file_path: analysis_result.file_path,
                    success: false,
                    renamed: false,
                    new_path: None,
                    error: Some(format!("Failed to {} file: {}", mode, e)),
                }
            }
        }
    }

    fn get_file_extension(&self, file_path: &Path) -> String {
        file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| {
                let ext = s.to_lowercase();
                // Handle malformed extensions
                match ext.as_str() {
                    "%jpg" | "%jpeg" => "jpg".to_string(),
                    "%mov" => "mov".to_string(),
                    "%mp4" => "mp4".to_string(),
                    _ => ext,
                }
            })
            .unwrap_or_else(|| "".to_string())
    }
}

#[derive(Debug, Clone)]
pub struct AnalysisResult {
    pub file_path: PathBuf,
    pub success: bool,
    pub error: Option<String>,
    pub exif_data: Option<ExifData>,
    pub new_filename: Option<String>,
}
