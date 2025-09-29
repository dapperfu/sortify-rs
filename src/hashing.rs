/**
 * Content hashing module for duplicate detection using xxhash
 */

use anyhow::{Context, Result};
use log::warn;
use std::collections::HashMap;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::Path;
use xxhash_rust::xxh3;

pub struct ContentHasher {
    chunk_size: usize,
}

impl ContentHasher {
    pub fn new() -> Self {
        Self {
            chunk_size: 65536, // 64KB chunks
        }
    }

    /// Calculate xxhash of file content for duplicate detection
    /// 
    /// Uses xxh3 algorithm for maximum performance with streaming
    pub fn calculate_file_hash(&self, file_path: &Path) -> Result<String> {
        let file = File::open(file_path)
            .context("Failed to open file for hashing")?;

        let mut reader = BufReader::new(file);
        let mut buffer = vec![0u8; self.chunk_size];
        let mut hasher = xxh3::Xxh3::default();
        
        loop {
            let bytes_read = reader.read(&mut buffer)
                .context("Failed to read file for hashing")?;
            
            if bytes_read == 0 {
                break;
            }
            
            hasher.update(&buffer[..bytes_read]);
        }

        let hash = hasher.digest();
        Ok(format!("{:016x}", hash))
    }

    /// Build an index of file content hashes for multiple files
    pub fn build_content_hash_index<'a>(
        &self,
        file_paths: &[&'a Path],
    ) -> Result<HashMap<&'a Path, String>> {
        let mut hash_index = HashMap::new();
        
        for file_path in file_paths {
            match self.calculate_file_hash(file_path) {
                Ok(hash) => {
                    hash_index.insert(*file_path, hash);
                }
                Err(e) => {
                    warn!("Failed to calculate hash for {}: {}", file_path.display(), e);
                }
            }
        }
        
        Ok(hash_index)
    }

    /// Build content hash index for directory structure
    pub fn build_content_hash_index_for_directory(
        &self,
        directory: &Path,
    ) -> Result<HashMap<String, String>> {
        let mut hash_index = HashMap::new();
        
        for entry in walkdir::WalkDir::new(directory).into_iter().filter_map(|e| e.ok()) {
            if entry.file_type().is_file() {
                if let Ok(rel_path) = entry.path().strip_prefix(directory) {
                    if let Ok(hash) = self.calculate_file_hash(entry.path()) {
                        hash_index.insert(
                            rel_path.to_string_lossy().to_string(),
                            hash
                        );
                    }
                }
            }
        }
        
        Ok(hash_index)
    }

    /// Check if a file is a content duplicate of an existing file
    pub fn is_content_duplicate(
        &self,
        file_path: &Path,
        existing_hash_index: &HashMap<String, String>,
        target_directory: &Path,
    ) -> Option<String> {
        let file_hash = match self.calculate_file_hash(file_path) {
            Ok(hash) => hash,
            Err(_) => return None,
        };

        // Check if this hash already exists in the index
        for (existing_path, existing_hash) in existing_hash_index {
            if existing_hash == &file_hash {
                // Verify it's not the same file
                let existing_full_path = target_directory.join(existing_path);
                if !file_path.canonicalize().ok()
                    .and_then(|canonical_file| existing_full_path.canonicalize().ok().map(|canonical_existing| canonical_file == canonical_existing))
                    .unwrap_or(false) {
                    return Some(existing_path.clone());
                }
            }
        }

        None
    }
}
