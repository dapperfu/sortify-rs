/**
 * EXIF processing module with fast-exif-rs implementation
 * 
 * Processing order (fastest to slowest):
 * 1. fast-exif-rs (ultra-fast pure Rust, 55.6x faster than standard libraries)
 * 2. kamadak-exif (pure Rust, good compatibility) 
 * 3. File modification time (last resort)
 */

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use exif::Reader as ExifReader;
use fast_exif_reader::{FastExifReader, UltraFastJpegReader, HybridExifReader, ExifError};
use log::{debug, warn};
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

    /// Extract EXIF data from file using fast-exif-rs with comprehensive fallback strategy
    /// 
    /// Processing order (fastest to slowest):
    /// 1. Ultra-fast JPEG reader (for JPEG files only, zero-copy optimization)
    /// 2. fast-exif-rs (ultra-fast pure Rust, works for all formats)
    /// 3. Hybrid reader (balanced performance and compatibility)
    /// 4. kamadak-exif (pure Rust, good compatibility) 
    /// 5. File modification time (last resort)
    pub fn extract_exif_data(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Processing file: {}", file_path.display());

        let file_ext = file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let is_video = matches!(file_ext.as_str(), "mov" | "mp4" | "avi" | "mkv" | "3gp" | "m4v");
        let is_jpeg = matches!(file_ext.as_str(), "jpg" | "jpeg");

        // Method 1: For JPEG files, try ultra-fast JPEG reader first (specialized zero-copy optimization)
        if is_jpeg {
            match self.extract_exif_data_ultra_fast_jpeg(file_path) {
                Ok(data) => {
                    debug!("ultra-fast JPEG reader succeeded for: {}", file_path.display());
                    return Ok(data);
                }
                Err(e) => {
                    debug!("ultra-fast JPEG reader failed for {}: {}", file_path.display(), e);
                }
            }
        }

        // Method 2: Try fast-exif-rs (ultra-fast pure Rust, works for all formats)
        match self.extract_exif_data_fast_exif(file_path) {
            Ok(data) => {
                debug!("fast-exif-rs succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("fast-exif-rs failed for {}: {}", file_path.display(), e);
            }
        }

        // Method 3: Try hybrid reader for better compatibility
        match self.extract_exif_data_hybrid(file_path) {
            Ok(data) => {
                debug!("hybrid reader succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("hybrid reader failed for {}: {}", file_path.display(), e);
            }
        }

        // Method 4: Try kamadak-exif (pure Rust, good compatibility)
        match self.extract_exif_data_kamadak(file_path) {
            Ok(data) => {
                debug!("kamadak-exif succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("kamadak-exif failed for {}: {}", file_path.display(), e);
            }
        }

        // Method 5: Use file modification time as last resort
        warn!("Using file modification time for: {}", file_path.display());
        self.extract_file_mtime(file_path)
    }

    /// Extract EXIF data using fast-exif-rs (ultra-fast pure Rust implementation)
    pub fn extract_exif_data_fast_exif(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Using fast-exif-rs for: {}", file_path.display());
        
        let mut fast_reader = FastExifReader::new();
        let file_path_str = file_path.to_string_lossy().to_string();
        let metadata = fast_reader.read_file(&file_path_str)
            .map_err(|e| anyhow::anyhow!("fast-exif-rs failed: {}", e))?;

        // Extract best timestamp
        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            metadata,
        })
    }

    /// Extract EXIF data using ultra-fast JPEG reader (specialized for JPEG files)
    pub fn extract_exif_data_ultra_fast_jpeg(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Using ultra-fast JPEG reader for: {}", file_path.display());
        
        let mut ultra_reader = UltraFastJpegReader::new();
        let file_path_str = file_path.to_string_lossy().to_string();
        let metadata = ultra_reader.read_file(&file_path_str)
            .map_err(|e| anyhow::anyhow!("ultra-fast JPEG reader failed: {}", e))?;

        // Extract best timestamp
        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            metadata,
        })
    }

    /// Extract EXIF data using hybrid reader (balanced performance and compatibility)
    pub fn extract_exif_data_hybrid(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Using hybrid reader for: {}", file_path.display());
        
        let mut hybrid_reader = HybridExifReader::new();
        let file_path_str = file_path.to_string_lossy().to_string();
        let metadata = hybrid_reader.read_file(&file_path_str)
            .map_err(|e| anyhow::anyhow!("hybrid reader failed: {}", e))?;

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
        
        // Handle EXIF format timestamps from kamadak-exif
        let (main_part, subsec_part) = if timestamp_str.contains('.') {
            // Standard EXIF format: 2025:09:24 08:20:49.680 or 2025-09-24 08:20:49.680
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
            // ISO format: 2025-09-24 08:20:49
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
        // Remove quotes if present
        let subsec_str = subsec_str.trim_matches('"');
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
