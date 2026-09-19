/**
 * File naming and organization module
 */

use chrono::{DateTime, Utc, Datelike, Timelike};
use std::collections::HashSet;
use std::path::Path;

pub struct FilenameGenerator {
    _existing_files: HashSet<String>,
}

impl FilenameGenerator {
    pub fn new() -> Self {
        Self {
            _existing_files: HashSet::new(),
        }
    }

    /// Unsuffixed destination path: YYYY/MM-Mon/YYYYMMDD_HHMMSS.fff.ext
    pub fn unsuffixed_relative_path(
        dt: DateTime<Utc>,
        milliseconds: u16,
        extension: &str,
    ) -> String {
        let year = dt.year();
        let month_num = dt.month();
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun",
            "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month_name = month_names[(month_num - 1) as usize];
        format!(
            "{}/{:02}-{}/{}{:02}{:02}_{:02}{:02}{:02}.{:03}.{}",
            year,
            month_num,
            month_name,
            year,
            month_num,
            dt.day(),
            dt.hour(),
            dt.minute(),
            dt.second(),
            milliseconds,
            extension
        )
    }

    /// Generate filename with subsecond precision.
    ///
    /// `-2`, `-3`, … are added only when that exact destination is already taken
    /// by another file in this batch or on disk (not the source itself).
    pub fn generate_filename(
        &self,
        dt: DateTime<Utc>,
        milliseconds: u16,
        extension: &str,
        existing_files: &[String],
        output_dir: Option<&Path>,
        source: Option<&Path>,
    ) -> String {
        let year = dt.year();
        let month_num = dt.month();
        let month_names = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun",
            "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let month_name = month_names[(month_num - 1) as usize];

        let mut final_path = Self::unsuffixed_relative_path(dt, milliseconds, extension);
        if !self.is_taken(&final_path, existing_files, output_dir, source) {
            return final_path;
        }

        let mut counter = 2;
        loop {
            final_path = format!(
                "{}/{:02}-{}/{}{:02}{:02}_{:02}{:02}{:02}.{:03}-{}.{}",
                year,
                month_num,
                month_name,
                year,
                month_num,
                dt.day(),
                dt.hour(),
                dt.minute(),
                dt.second(),
                milliseconds,
                counter,
                extension
            );
            if !self.is_taken(&final_path, existing_files, output_dir, source) {
                return final_path;
            }
            counter += 1;
        }
    }

    fn is_taken(
        &self,
        relative_path: &str,
        existing_files: &[String],
        output_dir: Option<&Path>,
        source: Option<&Path>,
    ) -> bool {
        if existing_files.iter().any(|p| p == relative_path) {
            return true;
        }
        let Some(output_dir) = output_dir else {
            return false;
        };
        let dest = output_dir.join(relative_path);
        if !dest.exists() {
            return false;
        }
        if let Some(source) = source {
            if paths_are_same(&dest, source) {
                return false;
            }
        }
        true
    }
}

pub fn paths_are_same(a: &Path, b: &Path) -> bool {
    if a == b {
        return true;
    }
    match (a.canonicalize(), b.canonicalize()) {
        (Ok(ca), Ok(cb)) => ca == cb,
        _ => false,
    }
}
