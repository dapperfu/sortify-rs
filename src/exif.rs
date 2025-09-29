/**
 * EXIF processing module with Rust-only implementation
 * 
 * Processing order (fastest to slowest):
 * 1. nom-exif (pure Rust, async support) - implemented
 * 2. kamadak-exif (pure Rust, good compatibility) - implemented
 * 3. File modification time (last resort) - implemented
 */

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use exif::Reader as ExifReader;
use log::{debug, warn};
use nom_exif::parse_exif;
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct ExifData {
    pub timestamp: DateTime<Utc>,
    pub milliseconds: u16,
    pub metadata: HashMap<String, String>,
}

pub struct ExifProcessor;

impl ExifProcessor {
    pub fn new() -> Self {
        Self
    }

    /// Extract EXIF data from file using Rust-only methods with fallback
    /// 
    /// Processing order (fastest to slowest):
    /// 1. nom-exif (pure Rust, async support)
    /// 2. kamadak-exif (pure Rust, good compatibility)
    /// 3. File modification time (last resort)
    pub fn extract_exif_data(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Processing file: {}", file_path.display());

        let file_ext = file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let is_video = matches!(file_ext.as_str(), "mov" | "mp4" | "avi" | "mkv" | "3gp" | "m4v");

        // For video files, try Rust parsers first, then fall back to file mtime
        if is_video {
            debug!("Processing video file with Rust parsers: {}", file_path.display());
            
            // Try nom-exif first for video files
            match self.extract_exif_data_nom_exif_blocking(file_path) {
                Ok(data) => {
                    debug!("nom-exif succeeded for video: {}", file_path.display());
                    return Ok(data);
                }
                Err(e) => {
                    debug!("nom-exif failed for video {}: {}", file_path.display(), e);
                }
            }

            // Try kamadak-exif for video files
            match self.extract_exif_data_kamadak(file_path) {
                Ok(data) => {
                    debug!("kamadak-exif succeeded for video: {}", file_path.display());
                    return Ok(data);
                }
                Err(e) => {
                    debug!("kamadak-exif failed for video {}: {}", file_path.display(), e);
                }
            }

            // For video files, fall back to file modification time
            warn!("Using file modification time for video: {}", file_path.display());
            return self.extract_file_mtime(file_path);
        }

        // Method 1: Try nom-exif first (pure Rust, async support)
        // For now, use blocking version in main method
        match self.extract_exif_data_nom_exif_blocking(file_path) {
            Ok(data) => {
                debug!("nom-exif succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("nom-exif failed for {}: {}", file_path.display(), e);
            }
        }

        // Method 2: Try kamadak-exif (pure Rust, good compatibility)
        match self.extract_exif_data_kamadak(file_path) {
            Ok(data) => {
                debug!("kamadak-exif succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("kamadak-exif failed for {}: {}", file_path.display(), e);
            }
        }

        // Method 3: Use file modification time as last resort
        warn!("Using file modification time for: {}", file_path.display());
        self.extract_file_mtime(file_path)
    }

    /// Extract EXIF data using nom-exif (pure Rust, async support)
    pub async fn extract_exif_data_nom_exif(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Using nom-exif (async) for: {}", file_path.display());
        
        let file = File::open(file_path)
            .context("Failed to open file for nom-exif")?;
        
        let iter = parse_exif(file, None)
            .context("Failed to parse EXIF data with nom-exif")?
            .ok_or_else(|| anyhow::anyhow!("No EXIF data found"))?;

        let mut metadata = HashMap::new();
        
        // Extract all EXIF fields
        for entry in iter.clone() {
            if let Some(tag) = entry.tag() {
                let tag_name = format!("{:?}", tag);
                let value = match entry.take_result() {
                    Ok(result) => result.to_string(),
                    Err(_) => "Error".to_string(),
                };
                metadata.insert(tag_name, value);
            }
        }

        // Extract best timestamp
        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            metadata,
        })
    }

    /// Extract EXIF data using nom-exif (blocking version for benchmarks)
    pub fn extract_exif_data_nom_exif_blocking(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Using nom-exif (blocking) for: {}", file_path.display());
        
        let file = File::open(file_path)
            .context("Failed to open file for nom-exif")?;
        
        let iter = parse_exif(file, None)
            .context("Failed to parse EXIF data with nom-exif")?
            .ok_or_else(|| anyhow::anyhow!("No EXIF data found"))?;

        let mut metadata = HashMap::new();
        
        // Extract all EXIF fields
        for entry in iter.clone() {
            if let Some(tag) = entry.tag() {
                let tag_name = format!("{:?}", tag);
                let value = match entry.take_result() {
                    Ok(result) => result.to_string(),
                    Err(_) => "Error".to_string(),
                };
                metadata.insert(tag_name, value);
            }
        }

        // Extract best timestamp
        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            metadata,
        })
    }

    pub fn extract_exif_data_kamadak(&self, file_path: &Path) -> Result<ExifData> {
        let file = File::open(file_path)
            .context("Failed to open file for kamadak-exif")?;
        let mut bufreader = BufReader::new(&file);
        
        let exifreader = ExifReader::new();
        let exif = exifreader.read_from_container(&mut bufreader)
            .context("Failed to read EXIF data with kamadak-exif")?;

        let mut metadata = HashMap::new();
        
        // Extract all EXIF fields
        for field in exif.fields() {
            let tag_name = field.tag.to_string();
            let value = field.display_value().with_unit(&exif).to_string();
            metadata.insert(tag_name, value);
        }

        // Extract best timestamp
        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            metadata,
        })
    }


    fn extract_file_mtime(&self, file_path: &Path) -> Result<ExifData> {
        let metadata = std::fs::metadata(file_path)
            .context("Failed to read file metadata")?;

        let mtime = metadata.modified()
            .context("Failed to get file modification time")?;

        let timestamp = DateTime::<Utc>::from(mtime);
        let mut metadata_map = HashMap::new();
        metadata_map.insert("ModifyDate".to_string(), timestamp.format("%Y:%m:%d %H:%M:%S").to_string());

        Ok(ExifData {
            timestamp,
            milliseconds: 0,
            metadata: metadata_map,
        })
    }

    /// Extract the best available timestamp from EXIF data using comprehensive fallback hierarchy
    /// 
    /// Priority order for photos:
    /// 1. SubSecCreateDate (with subseconds)
    /// 2. SubSecDateTimeOriginal (with subseconds) 
    /// 3. SubSecModifyDate (with subseconds)
    /// 4. DateTimeOriginal + SubSecTimeOriginal (combined)
    /// 5. ModifyDate + SubSecTime (combined)
    /// 6. DateTimeDigitized + SubSecTimeDigitized (combined)
    /// 7. DateTimeOriginal (fallback)
    /// 8. ModifyDate (fallback)
    /// 9. DateTimeDigitized (fallback)
    /// 10. CreateDate (LAST RESORT)
    /// 
    /// Priority order for videos:
    /// 1. DateTimeOriginal
    /// 2. NikonDateTime
    /// 3. MediaCreateDate
    /// 4. MediaModifyDate
    /// 5. ModifyDate
    /// 6. CreateDate (LAST RESORT)
    fn extract_best_timestamp(&self, metadata: &HashMap<String, String>) -> Result<(DateTime<Utc>, u16)> {
        // Check if this is a video file
        let is_video = metadata.contains_key("MediaCreateDate") || metadata.contains_key("MediaModifyDate");

        if is_video {
            self.extract_video_timestamp(metadata)
        } else {
            self.extract_photo_timestamp(metadata)
        }
    }

    fn extract_video_timestamp(&self, metadata: &HashMap<String, String>) -> Result<(DateTime<Utc>, u16)> {
        let timestamp_fields = [
            "DateTimeOriginal",
            "NikonDateTime", 
            "MediaCreateDate",
            "MediaModifyDate",
            "ModifyDate",
        ];

        for field in timestamp_fields {
            if let Some(timestamp_str) = metadata.get(field) {
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(timestamp_str) {
                    return Ok((dt, ms));
                }
            }
        }

        // LAST RESORT: CreateDate
        if let Some(timestamp_str) = metadata.get("CreateDate") {
            if !self.is_zero_timestamp(timestamp_str) {
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(timestamp_str) {
                    return Ok((dt, ms));
                }
            }
        }

        anyhow::bail!("No valid timestamp found in video EXIF data");
    }

    fn extract_photo_timestamp(&self, metadata: &HashMap<String, String>) -> Result<(DateTime<Utc>, u16)> {
        // 1. Pre-combined subsecond timestamps (highest priority)
        let pre_combined_fields = [
            "SubSecCreateDate",
            "SubSecDateTimeOriginal", 
            "SubSecModifyDate"
        ];

        for field in pre_combined_fields {
            if let Some(timestamp_str) = metadata.get(field) {
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(timestamp_str) {
                    return Ok((dt, ms));
                }
            }
        }

        // 2. Combine base timestamps with subsecond data
        let combinations = [
            ("DateTimeOriginal", "SubSecTimeOriginal"),
            ("ModifyDate", "SubSecTime"),
            ("DateTimeDigitized", "SubSecTimeDigitized")
        ];

        for (base_field, subsec_field) in combinations {
            if let (Some(base_time), Some(subsec_value)) = (metadata.get(base_field), metadata.get(subsec_field)) {
                let padded_subsec = format!("{:0<3}", subsec_value);
                let combined_timestamp = format!("{}.{}", base_time, padded_subsec);
                
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(&combined_timestamp) {
                    return Ok((dt, ms));
                }
            }
        }

        // 3. Fallback to base timestamps only
        let fallback_fields = [
            "DateTimeOriginal",
            "ModifyDate",
            "DateTimeDigitized"
        ];

        for field in fallback_fields {
            if let Some(timestamp_str) = metadata.get(field) {
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(timestamp_str) {
                    return Ok((dt, ms));
                }
            }
        }

        // 4. LAST RESORT: CreateDate
        if let Some(timestamp_str) = metadata.get("CreateDate") {
            if !self.is_zero_timestamp(timestamp_str) {
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(timestamp_str) {
                    return Ok((dt, ms));
                }
            }
        }

        anyhow::bail!("No valid timestamp found in photo EXIF data");
    }

    fn parse_timestamp_with_subseconds(&self, timestamp_str: &str) -> Result<(DateTime<Utc>, u16)> {
        let timestamp_str = timestamp_str.trim();
        
        // Handle different timestamp formats
        let (main_part, subsec_part) = if timestamp_str.contains('T') {
            // ISO format: 2025-09-24T08:20:49-04:00 or 2025-09-24T08:20:49
            let parts: Vec<&str> = timestamp_str.split('T').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid ISO timestamp format: {}", timestamp_str);
            }
            let date_part = parts[0];
            let time_part = parts[1];
            
            // Remove timezone info from time part
            let time_part = if time_part.len() > 6 {
                let last_6 = &time_part[time_part.len()-6..];
                if last_6.starts_with('+') || last_6.starts_with('-') {
                    &time_part[..time_part.len()-6]
                } else {
                    time_part
                }
            } else {
                time_part
            };
            
            let combined = format!("{} {}", date_part, time_part);
            if combined.contains('.') {
                let subsec_parts: Vec<&str> = combined.split('.').collect();
                if subsec_parts.len() != 2 {
                    anyhow::bail!("Invalid timestamp format: {}", timestamp_str);
                }
                (subsec_parts[0].to_string(), subsec_parts[1].to_string())
            } else {
                (combined, "0".to_string())
            }
        } else if timestamp_str.contains('.') {
            // Standard EXIF format: 2025:09:24 08:20:49.680
            let parts: Vec<&str> = timestamp_str.split('.').collect();
            if parts.len() != 2 {
                anyhow::bail!("Invalid timestamp format: {}", timestamp_str);
            }
            (parts[0].to_string(), parts[1].to_string())
        } else {
            // No subseconds
            (timestamp_str.to_string(), "0".to_string())
        };

        // Parse the main datetime part - try different formats
        let naive_dt = if main_part.contains('-') {
            // ISO date format: 2025-09-24 08:20:49
            NaiveDateTime::parse_from_str(&main_part, "%Y-%m-%d %H:%M:%S")
                .context("Failed to parse ISO timestamp")?
        } else {
            // EXIF format: 2025:09:24 08:20:49
            NaiveDateTime::parse_from_str(&main_part, "%Y:%m:%d %H:%M:%S")
                .context("Failed to parse EXIF timestamp")?
        };

        // Parse subseconds and convert to milliseconds
        let subsec_str = if subsec_part.len() > 3 {
            &subsec_part[..3]
        } else {
            &subsec_part
        };
        let padded_subsec = format!("{:0<3}", subsec_str);
        let milliseconds: u16 = padded_subsec.parse()
            .context("Failed to parse subseconds")?;

        let dt = DateTime::<Utc>::from_naive_utc_and_offset(naive_dt, Utc);
        Ok((dt, milliseconds))
    }

    fn is_zero_timestamp(&self, timestamp_str: &str) -> bool {
        timestamp_str.replace(':', "").replace(' ', "").replace('0', "").is_empty()
    }
}
