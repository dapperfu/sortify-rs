/**
 * EXIF processing module with fast-exif-rs implementation
 * 
 * Processing order:
 * 1. fast-exif-rs datetime read (v0.11.2 composites: SubSec*, CreateDate aliases)
 * 2. fast-exif-rs full read (remaining video/maker-note dates)
 */

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc};
use fast_exif_reader::{FastExifReader, ReadOptions};
use log::debug;
use std::collections::HashMap;
use std::path::Path;

/// Simple EXIF writer for basic tag writing
struct ExifWriter {
    tags: HashMap<String, String>,
}

impl ExifWriter {
    fn new() -> Self {
        Self {
            tags: HashMap::new(),
        }
    }
    
    fn add_ascii_tag(&mut self, name: &str, value: &str) -> Result<()> {
        self.tags.insert(name.to_string(), value.to_string());
        Ok(())
    }
    
    fn add_short_tag(&mut self, name: &str, value: u16) -> Result<()> {
        self.tags.insert(name.to_string(), value.to_string());
        Ok(())
    }
    
    fn add_long_tag(&mut self, name: &str, value: u32) -> Result<()> {
        self.tags.insert(name.to_string(), value.to_string());
        Ok(())
    }
    
    fn write_to_jpeg(&self, file_path: &Path) -> Result<()> {
        debug!("Writing EXIF data to JPEG file: {}", file_path.display());
        
        // Read the JPEG file
        let mut file_data = std::fs::read(file_path)
            .context("Failed to read JPEG file")?;
        
        // Create simple EXIF data
        let exif_data = self.create_simple_exif_data()?;
        
        // Create APP1 segment with EXIF data
        let app1_segment = self.create_app1_segment(&exif_data)?;
        
        // Insert or replace APP1 segment in JPEG
        self.insert_app1_segment(&mut file_data, &app1_segment)?;
        
        // Write back to file
        std::fs::write(file_path, &file_data)
            .context("Failed to write JPEG file")?;
        
        debug!("Successfully wrote EXIF data to JPEG file");
        Ok(())
    }
    
    fn write_to_tiff(&self, file_path: &Path) -> Result<()> {
        debug!("Writing EXIF data to TIFF file: {}", file_path.display());
        
        // Create simple TIFF data
        let tiff_data = self.create_simple_tiff_data()?;
        
        // Write TIFF data to file
        std::fs::write(file_path, &tiff_data)
            .context("Failed to write TIFF file")?;
        
        debug!("Successfully wrote EXIF data to TIFF file");
        Ok(())
    }
    
    fn create_simple_exif_data(&self) -> Result<Vec<u8>> {
        // Create a minimal EXIF structure
        let mut data = Vec::new();
        
        // TIFF header (little-endian)
        data.extend_from_slice(b"II"); // Little-endian
        data.extend_from_slice(&42u16.to_le_bytes()); // Magic number
        data.extend_from_slice(&8u32.to_le_bytes()); // Offset to first IFD
        
        // Simple IFD with our tags
        let tag_count = self.tags.len() as u16;
        data.extend_from_slice(&tag_count.to_le_bytes());
        
        // Add IFD entries for each tag
        for (tag_name, tag_value) in &self.tags {
            self.add_ifd_entry(&mut data, tag_name, tag_value)?;
        }
        
        // Next IFD offset (0 = end)
        data.extend_from_slice(&0u32.to_le_bytes());
        
        Ok(data)
    }
    
    fn create_simple_tiff_data(&self) -> Result<Vec<u8>> {
        // Same as EXIF data for TIFF
        self.create_simple_exif_data()
    }
    
    fn add_ifd_entry(&self, data: &mut Vec<u8>, tag_name: &str, tag_value: &str) -> Result<()> {
        // Map tag names to IDs (simplified)
        let tag_id: u16 = match tag_name {
            "DateTime" => 0x0132,
            "DateTimeOriginal" => 0x9003,
            "DateTimeDigitized" => 0x9004,
            "Artist" => 0x013B,
            "Copyright" => 0x8298,
            _ => 0x010E, // ImageDescription as default
        };
        
        // Tag ID (2 bytes)
        data.extend_from_slice(&tag_id.to_le_bytes());
        
        // Tag type: ASCII = 2 (2 bytes)
        data.extend_from_slice(&2u16.to_le_bytes());
        
        // Count: length of string + null terminator (4 bytes)
        let count = tag_value.len() + 1;
        data.extend_from_slice(&(count as u32).to_le_bytes());
        
        // Value: ASCII string (4 bytes, padded)
        let mut value_bytes = tag_value.as_bytes().to_vec();
        value_bytes.push(0); // Null terminator
        while value_bytes.len() < 4 {
            value_bytes.push(0);
        }
        data.extend_from_slice(&value_bytes[..4]);
        
        Ok(())
    }
    
    fn create_app1_segment(&self, exif_data: &[u8]) -> Result<Vec<u8>> {
        let mut segment = Vec::new();
        
        // APP1 marker (0xFFE1)
        segment.push(0xFF);
        segment.push(0xE1);
        
        // Calculate segment length (2 bytes for length + 6 bytes for "Exif\0\0" + EXIF data)
        let segment_length = 2 + 6 + exif_data.len();
        if segment_length > 65535 {
            anyhow::bail!("EXIF data too large for JPEG APP1 segment");
        }
        
        // Write segment length (big-endian)
        segment.push((segment_length >> 8) as u8);
        segment.push(segment_length as u8);
        
        // Write "Exif\0\0" identifier
        segment.extend_from_slice(b"Exif\0\0");
        
        // Write EXIF data
        segment.extend_from_slice(exif_data);
        
        Ok(segment)
    }
    
    fn insert_app1_segment(&self, jpeg_data: &mut Vec<u8>, app1_segment: &[u8]) -> Result<()> {
        // Find existing APP1 segment and replace it, or insert after SOI marker
        let mut insert_pos = None;
        let mut remove_start = None;
        let mut remove_end = None;
        
        let mut i = 0;
        while i < jpeg_data.len() - 1 {
            if jpeg_data[i] == 0xFF {
                match jpeg_data[i + 1] {
                    0xD8 => { // SOI marker
                        insert_pos = Some(i + 2);
                        i += 2;
                        continue;
                    }
                    0xE1 => { // APP1 marker
                        // Found existing APP1 segment, mark for removal
                        if i + 3 < jpeg_data.len() {
                            let length = ((jpeg_data[i + 2] as u16) << 8) | (jpeg_data[i + 3] as u16);
                            remove_start = Some(i);
                            remove_end = Some(i + 2 + length as usize);
                            i += 2 + length as usize;
                            continue;
                        }
                    }
                    0xD9 => { // EOI marker - end of image
                        break;
                    }
                    _ => {
                        // Other marker, skip it
                        if i + 3 < jpeg_data.len() {
                            let length = ((jpeg_data[i + 2] as u16) << 8) | (jpeg_data[i + 3] as u16);
                            i += 2 + length as usize;
                            continue;
                        }
                    }
                }
            }
            i += 1;
        }
        
        // Remove existing APP1 segment if found
        if let (Some(start), Some(end)) = (remove_start, remove_end) {
            jpeg_data.drain(start..end);
        }
        
        // Insert new APP1 segment
        if let Some(pos) = insert_pos {
            // Adjust position if we removed a segment
            let adjusted_pos = if let Some(remove_start) = remove_start {
                if pos > remove_start {
                    pos - (remove_end.unwrap() - remove_start)
                } else {
                    pos
                }
            } else {
                pos
            };
            
            jpeg_data.splice(adjusted_pos..adjusted_pos, app1_segment.iter().cloned());
        } else {
            anyhow::bail!("Invalid JPEG file: SOI marker not found");
        }
        
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct ExifData {
    pub timestamp: DateTime<Utc>,
    pub milliseconds: u16,
    pub _metadata: HashMap<String, String>,
}

pub struct ExifProcessor {
    /// Reused reader; v0.11.2 materializes extra date aliases and SubSec composites.
    reader: FastExifReader,
    /// Essential fields for timestamp extraction only
    _essential_fields: Vec<String>,
}

impl ExifProcessor {
    pub fn new() -> Self {
        // Define essential fields for timestamp extraction only
        let essential_fields = vec![
            "Make".to_string(),
            "Model".to_string(),
            "DateTime".to_string(),
            "DateTimeOriginal".to_string(),
            "DateTimeDigitized".to_string(),
            "ModifyDate".to_string(),
            "CreateDate".to_string(),
            "SubSecTime".to_string(),
            "SubSecTimeOriginal".to_string(),
            "SubSecTimeDigitized".to_string(),
            "SubSecCreateDate".to_string(),
            "SubSecModifyDate".to_string(),
            "SubSecDateTimeOriginal".to_string(),
            "OffsetTime".to_string(),
            "OffsetTimeOriginal".to_string(),
            "OffsetTimeDigitized".to_string(),
            "DateTimeCreated".to_string(),
        ];

        Self {
            reader: FastExifReader::new(),
            _essential_fields: essential_fields,
        }
    }

    /// Extract EXIF data from file using optimized fast-exif-rs with intelligent parser selection
    /// 
    /// Processing order (fastest to slowest):
    /// 1. Optimal EXIF parser (automatic optimization with ultra-seek, memory mapping, SIMD)
    /// 2. fast-exif-rs (ultra-fast pure Rust, works for all formats)
    /// 
    /// Note: File modification time fallback has been removed as it's unreliable.
    /// Files without valid EXIF timestamps will be ignored.
    pub fn extract_exif_data(&mut self, file_path: &Path) -> Result<ExifData> {
        debug!("Processing file: {}", file_path.display());

        // Get file size to determine optimal parsing strategy
        let _file_size = std::fs::metadata(file_path)
            .map(|m| m.len())
            .unwrap_or(0);

        let file_ext = file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let _is_jpeg = matches!(file_ext.as_str(), "jpg" | "jpeg");

        // Method 1: Date-focused parse (composite SubSec* / CreateDate from v0.11.2)
        match self.extract_exif_data_optimal(file_path) {
            Ok(data) => {
                debug!("datetime parser succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("datetime parser failed for {}: {}", file_path.display(), e);
            }
        }

        // Method 2: Full tag set for formats that only expose video/maker-note dates
        match self.extract_exif_data_fast_exif(file_path) {
            Ok(data) => {
                debug!("fast-exif-rs succeeded for: {}", file_path.display());
                return Ok(data);
            }
            Err(e) => {
                debug!("fast-exif-rs failed for {}: {}", file_path.display(), e);
            }
        }

        // No valid EXIF timestamp found - ignore the file
        anyhow::bail!("No valid EXIF timestamp found for: {}", file_path.display())
    }

    /// Date-focused read: v0.11.2 fills CreateDate aliases and SubSec* composites.
    fn datetime_read_options() -> ReadOptions {
        let mut opts = ReadOptions::tags([
            "DateTimeOriginal",
            "CreateDate",
            "DateTime",
            "ModifyDate",
            "DateTimeDigitized",
            "DateTimeCreated",
            "CreationDate",
            "SubSecTime",
            "SubSecTimeOriginal",
            "SubSecTimeDigitized",
            "OffsetTime",
            "OffsetTimeOriginal",
            "OffsetTimeDigitized",
            "SubSecCreateDate",
            "SubSecDateTimeOriginal",
            "SubSecModifyDate",
            "MediaCreateDate",
            "TrackCreateDate",
            "MediaModifyDate",
            "TrackModifyDate",
            "NikonDateTime",
        ]);
        opts.include_computed_fields = true;
        opts
    }

    /// Extract EXIF data using the datetime-optimized fast-exif-rs path
    pub fn extract_exif_data_optimal(&mut self, file_path: &Path) -> Result<ExifData> {
        debug!("Using fast-exif-rs datetime options for: {}", file_path.display());

        let file_path_str = file_path.to_string_lossy().to_string();
        let metadata = self
            .reader
            .read_file_with_options(&file_path_str, &Self::datetime_read_options())
            .map_err(|e| anyhow::anyhow!("datetime parser failed: {}", e))?;

        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            _metadata: metadata,
        })
    }

    /// Analyze a single file and return analysis result
    pub fn analyze_single_file(&mut self, file_path: &Path) -> crate::file_ops::AnalysisResult {
        // Skip symlinks
        if file_path.is_symlink() {
            return crate::file_ops::AnalysisResult {
                file_path: file_path.to_path_buf(),
                success: true,  // Mark as success so it shows up in skipped files
                exif_data: None,
                error: Some("Skipped symlink - cannot process symbolic links".to_string()),
                new_filename: None,
            };
        }

        // Try to extract EXIF data
        match self.extract_exif_data(file_path) {
            Ok(exif_data) => {
                // Generate filename for the extracted EXIF data
                let extension = self.get_file_extension(file_path);
                debug!("Generated extension: '{}' for file: {}", extension, file_path.display());
                debug!("EXIF timestamp: {} ({}ms)", exif_data.timestamp, exif_data.milliseconds);
                
                let new_filename = crate::naming::FilenameGenerator::unsuffixed_relative_path(
                    exif_data.timestamp,
                    exif_data.milliseconds,
                    &extension,
                );
                
                debug!("Generated filename: '{}'", new_filename);

                crate::file_ops::AnalysisResult {
                    file_path: file_path.to_path_buf(),
                    success: true,
                    exif_data: Some(exif_data),
                    error: None,
                    new_filename: Some(new_filename),
                }
            }
            Err(e) => {
                crate::file_ops::AnalysisResult {
                    file_path: file_path.to_path_buf(),
                    success: false,
                    exif_data: None,
                    error: Some(e.to_string()),
                    new_filename: None,
                }
            }
        }
    }

    /// Get file extension helper method
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

    /// Extract EXIF data using fast-exif-rs (ultra-fast pure Rust implementation)
    pub fn extract_exif_data_fast_exif(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Using fast-exif-rs for: {}", file_path.display());
        
        // Reuse reader instance to reduce allocation overhead
        let mut fast_reader = FastExifReader::new();
        let file_path_str = file_path.to_string_lossy().to_string();
        
        // Use optimized file reading with minimal buffering
        let metadata = fast_reader.read_file(&file_path_str)
            .map_err(|e| anyhow::anyhow!("fast-exif-rs failed: {}", e))?;

        // Extract best timestamp
        let (timestamp, milliseconds) = self.extract_best_timestamp(&metadata)?;

        Ok(ExifData {
            timestamp,
            milliseconds,
            _metadata: metadata,
        })
    }

    /// Pick a capture timestamp from EXIF tags.
    ///
    /// Order: SubSecDateTimeOriginal, SubSecModifyDate, DateTimeOriginal,
    /// DateTimeDigitized, DateTime, then every other parseable date tag
    /// alphabetically. Filesystem File* dates are ignored.
    fn extract_best_timestamp(&self, metadata: &HashMap<String, String>) -> Result<(DateTime<Utc>, u16)> {
        const PRIORITY: &[&str] = &[
            "SubSecDateTimeOriginal",
            "SubSecModifyDate",
            "DateTimeOriginal",
            "DateTimeDigitized",
            "DateTime",
        ];

        for field in PRIORITY {
            if let Some(timestamp_str) = metadata.get(*field) {
                match self.parse_timestamp_with_subseconds(timestamp_str) {
                    Ok((dt, ms)) => {
                        debug!("Using {}: {} ({}ms)", field, dt, ms);
                        return Ok((dt, ms));
                    }
                    Err(e) => debug!("Failed to parse {}: {}", field, e),
                }
            }
        }

        let mut rest: Vec<&String> = metadata
            .keys()
            .filter(|k| !PRIORITY.iter().any(|p| p == k))
            .filter(|k| !k.split(':').last().unwrap_or(k).starts_with("File"))
            .collect();
        rest.sort();

        for field in rest {
            if let Some(timestamp_str) = metadata.get(field) {
                match self.parse_timestamp_with_subseconds(timestamp_str) {
                    Ok((dt, ms)) => {
                        debug!("Using {}: {} ({}ms)", field, dt, ms);
                        return Ok((dt, ms));
                    }
                    Err(_) => {}
                }
            }
        }

        anyhow::bail!("No valid timestamp found in EXIF data");
    }

    /// Write EXIF data to a file
    /// 
    /// This method creates new EXIF data or modifies existing EXIF data in image files.
    /// Currently supports basic EXIF tag writing with plans for full JPEG/TIFF support.
    pub fn write_exif_data(&self, file_path: &Path, tags: HashMap<String, String>) -> Result<()> {
        debug!("Writing EXIF data to file: {}", file_path.display());
        
        let file_ext = file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

        let mut writer = ExifWriter::new();
        
        // Add all provided tags to the writer
        for (tag_name, tag_value) in tags {
            self.add_tag_to_writer(&mut writer, &tag_name, &tag_value)?;
        }
        
        // Write based on file format
        match file_ext.as_str() {
            "jpg" | "jpeg" => writer.write_to_jpeg(file_path),
            "tiff" | "tif" => writer.write_to_tiff(file_path),
            _ => anyhow::bail!("Unsupported file format for EXIF writing: {}", file_ext),
        }
    }

    /// Write a timestamp to EXIF data
    /// 
    /// This is a convenience method for updating timestamp-related EXIF tags.
    pub fn _write_timestamp(&self, file_path: &Path, timestamp: DateTime<Utc>) -> Result<()> {
        debug!("Writing timestamp to file: {}", file_path.display());
        
        let mut tags = HashMap::new();
        let formatted_time = timestamp.format("%Y:%m:%d %H:%M:%S").to_string();
        
        // Add multiple timestamp fields for maximum compatibility
        tags.insert("DateTime".to_string(), formatted_time.clone());
        tags.insert("DateTimeOriginal".to_string(), formatted_time.clone());
        tags.insert("DateTimeDigitized".to_string(), formatted_time);
        
        self.write_exif_data(file_path, tags)
    }

    /// Add a tag to the EXIF writer based on its type
    fn add_tag_to_writer(&self, writer: &mut ExifWriter, tag_name: &str, tag_value: &str) -> Result<()> {
        // Try to determine the appropriate tag type based on the value
        if let Ok(short_val) = tag_value.parse::<u16>() {
            writer.add_short_tag(tag_name, short_val)?;
        } else if let Ok(long_val) = tag_value.parse::<u32>() {
            writer.add_long_tag(tag_name, long_val)?;
        } else {
            // Default to ASCII string
            writer.add_ascii_tag(tag_name, tag_value)?;
        }
        Ok(())
    }

    /// Create a backup of the original file before writing EXIF data
    pub fn write_exif_data_with_backup(&self, file_path: &Path, tags: HashMap<String, String>) -> Result<()> {
        debug!("Writing EXIF data with backup for: {}", file_path.display());
        
        // Create backup filename
        let backup_path = file_path.with_extension(format!("{}.bak", 
            file_path.extension()
                .and_then(|ext| ext.to_str())
                .unwrap_or("")
        ));
        
        // Copy original to backup
        std::fs::copy(file_path, &backup_path)
            .context("Failed to create backup file")?;
        
        debug!("Created backup: {}", backup_path.display());
        
        // Write EXIF data
        match self.write_exif_data(file_path, tags) {
            Ok(()) => {
                debug!("Successfully wrote EXIF data to: {}", file_path.display());
                Ok(())
            }
            Err(e) => {
                // Restore from backup on failure
                std::fs::copy(&backup_path, file_path)
                    .context("Failed to restore from backup after EXIF write failure")?;
                Err(e.context("EXIF write failed, restored from backup"))
            }
        }
    }

    pub fn parse_timestamp_with_subseconds(&self, timestamp_str: &str) -> Result<(DateTime<Utc>, u16)> {
        let timestamp_str = timestamp_str.trim();
        
        // Handle EXIF format timestamps with timezone information
        // Format: 2025:10:12 16:26:03.12-04:00 or 2025:09:24 08:20:49.680
        let (main_part, subsec_part) = if timestamp_str.contains('.') {
            // Find the last dot before any timezone info
            let dot_pos = timestamp_str.rfind('.').unwrap();
            let after_dot = &timestamp_str[dot_pos + 1..];
            
            // Check if there's timezone info after the subseconds
            let timezone_pos = after_dot.find(|c: char| c == '+' || c == '-');
            
            if let Some(tz_pos) = timezone_pos {
                // Has timezone info: 2025:10:12 16:26:03.12-04:00
                let subsec_with_tz = &timestamp_str[dot_pos + 1..];
                let subsec_part = &subsec_with_tz[..tz_pos];
                let main_part = &timestamp_str[..dot_pos];
                (main_part.to_string(), subsec_part.to_string())
            } else {
                // No timezone info: 2025:09:24 08:20:49.680
                let parts: Vec<&str> = timestamp_str.split('.').collect();
                if parts.len() != 2 {
                    anyhow::bail!("Invalid timestamp format: {}", timestamp_str);
                }
                (parts[0].to_string(), parts[1].to_string())
            }
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

    fn _is_zero_timestamp(&self, timestamp_str: &str) -> bool {
        timestamp_str.replace(':', "").replace(' ', "").replace('0', "").is_empty()
    }
}
