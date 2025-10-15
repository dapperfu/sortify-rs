/**
 * File naming and organization module
 */

use chrono::{DateTime, Utc, Datelike, Timelike};
use std::collections::HashSet;

pub struct FilenameGenerator {
    _existing_files: HashSet<String>,
}

impl FilenameGenerator {
    pub fn new() -> Self {
        Self {
            _existing_files: HashSet::new(),
        }
    }

    /// Generate filename with subsecond precision and tie-breaking
    /// 
    /// Format: YYYY/MM-Mon/YYYYMMDD_HHMMSS.fff<ext>
    /// Tie-breaking: Files with identical timestamps get -2, -3, etc. suffixes
    pub fn generate_filename(
        &self,
        dt: DateTime<Utc>,
        milliseconds: u16,
        extension: &str,
        existing_files: &[String],
    ) -> String {
        let year = dt.year();
        let month_num = dt.month();
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun",
            "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"
        ];
        let month_name = month_names[(month_num - 1) as usize];

        let base_filename = format!(
            "{}{:02}{:02}_{:02}{:02}{:02}.{:03}.{}",
            year, month_num, dt.day(),
            dt.hour(), dt.minute(), dt.second(),
            milliseconds, extension
        );

        let full_path = format!("{}/{:02}-{}/{}", year, month_num, month_name, base_filename);

        // Check for ties and add suffix if needed
        let mut counter = 2;
        let mut final_path = full_path.clone();
        
        while existing_files.contains(&final_path) {
            let base_filename_with_suffix = format!(
                "{}{:02}{:02}_{:02}{:02}{:02}.{:03}-{}.{}",
                year, month_num, dt.day(),
                dt.hour(), dt.minute(), dt.second(),
                milliseconds, counter, extension
            );
            final_path = format!("{}/{:02}-{}/{}", year, month_num, month_name, base_filename_with_suffix);
            counter += 1;
        }

        final_path
    }

    /// Generate filename with content-based duplicate checking
    pub fn _generate_filename_with_duplicate_check(
        &self,
        dt: DateTime<Utc>,
        milliseconds: u16,
        extension: &str,
        file_path: &std::path::Path,
        existing_files: &[String],
        existing_hash_index: &std::collections::HashMap<std::path::PathBuf, String>,
        target_directory: &std::path::Path,
    ) -> (String, bool) {
        // Check for content duplicates first
        if let Some(_duplicate_path) = self._check_content_duplicate(file_path, existing_hash_index, target_directory) {
            return ("".to_string(), true);
        }

        // Generate filename normally if not a duplicate
        let filename = self.generate_filename(dt, milliseconds, extension, existing_files);
        (filename, false)
    }

    fn _check_content_duplicate(
        &self,
        _file_path: &std::path::Path,
        _existing_hash_index: &std::collections::HashMap<std::path::PathBuf, String>,
        _target_directory: &std::path::Path,
    ) -> Option<std::path::PathBuf> {
        // This would need to be implemented with the content hasher
        // For now, return None to indicate no duplicate found
        None
    }
}
