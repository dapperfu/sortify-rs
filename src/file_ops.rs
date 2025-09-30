/**
 * File operations module for processing and renaming files
 */

use anyhow::{Context, Result};
use indicatif::{ProgressBar, ProgressStyle};
use log::{info, warn};
use rayon::prelude::*;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use crate::exif::{ExifData, ExifProcessor};
use crate::hashing::ContentHasher;
use crate::naming::FilenameGenerator;

/// Perform file operation based on mode
fn perform_file_operation(source_path: &Path, target_path: &Path, mode: &str) -> Result<()> {
    match mode {
        "move" => {
            fs::rename(source_path, target_path)
                .context("Failed to move file")?;
        }
        "copy" => {
            fs::copy(source_path, target_path)
                .context("Failed to copy file")?;
        }
        "symlink" => {
            std::os::unix::fs::symlink(source_path, target_path)
                .context("Failed to create symlink")?;
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

        // Create output directory if it doesn't exist
        fs::create_dir_all(output_dir)
            .context("Failed to create output directory")?;

        // First pass: Extract EXIF data and generate filenames in parallel
        let analysis_results = self.analyze_files_parallel(files.clone())?;

        // Build content hash index for duplicate detection
        let hash_index = self.build_content_hash_index(&analysis_results, output_dir)?;

        // Second pass: Handle file operations sequentially to avoid conflicts
        let results = self.rename_files_sequential(analysis_results, &hash_index, output_dir, mode)?;

        Ok(results)
    }

    fn analyze_files_parallel(&self, files: Vec<PathBuf>) -> Result<Vec<AnalysisResult>> {
        let pb = ProgressBar::new(files.len() as u64);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{elapsed_precise}] [{bar:40.cyan/blue}] {pos}/{len} ({per_sec:.1} files/s) ETA: {eta} {msg}")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message("Analyzing files");

        // Use parallel processing with rayon for maximum performance
        let pb = Arc::new(pb);
        let results: Vec<AnalysisResult> = files
            .into_par_iter()
            .map(|file_path| {
                let result = self.analyze_single_file(&file_path);
                pb.inc(1);
                result
            })
            .collect();

        pb.finish_with_message("Analysis complete");
        Ok(results)
    }

    fn analyze_single_file(&self, file_path: &Path) -> AnalysisResult {
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
                let new_filename = self.filename_generator.generate_filename(
                    exif_data.timestamp,
                    exif_data.milliseconds,
                    &extension,
                    &[], // Will be updated with existing files later
                );

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

    fn rename_files_sequential(
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

        let mut results = Vec::new();
        let mut existing_files = Vec::new();

        for result in analysis_results {
            let process_result = self.process_single_file_rename(
                result,
                hash_index,
                output_dir,
                &mut existing_files,
                mode,
            );

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

            results.push(process_result);
            pb.inc(1);
        }

        pb.finish_with_message("Renaming complete");
        Ok(results)
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

        let (exif_data, new_filename) = match (analysis_result.exif_data, analysis_result.new_filename) {
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
struct AnalysisResult {
    file_path: PathBuf,
    success: bool,
    error: Option<String>,
    exif_data: Option<ExifData>,
    new_filename: Option<String>,
}
