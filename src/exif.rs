/**
 * EXIF processing module with fast-exif-rs implementation
 * 
 * Processing order (fastest to slowest):
 * 1. fast-exif-rs (ultra-fast pure Rust, 55.6x faster than standard libraries)
 * 2. kamadak-exif (pure Rust, good compatibility) 
 * 3. File modification time (last resort)
 */

use anyhow::{Context, Result};
use chrono::{DateTime, NaiveDateTime, Utc, Datelike};
use exif::Reader as ExifReader;
use fast_exif_reader::{FastExifReader, UltraFastJpegReader, HybridExifReader};
use log::{debug, warn};
use std::collections::HashMap;
use std::fs::File;
use std::io::BufReader;
use std::path::Path;

// EXIF Writer implementation inline
use std::io::Write;

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
    /// 
    /// Note: File modification time fallback has been removed as it's unreliable.
    /// Files without valid EXIF timestamps will be ignored.
    pub fn extract_exif_data(&self, file_path: &Path) -> Result<ExifData> {
        debug!("Processing file: {}", file_path.display());

        let file_ext = file_path.extension()
            .and_then(|ext| ext.to_str())
            .map(|s| s.to_lowercase())
            .unwrap_or_default();

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

        // No valid EXIF timestamp found - ignore the file
        anyhow::bail!("No valid EXIF timestamp found for: {}", file_path.display())
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
        // Priority order for video timestamps (avoiding unreliable file system dates)
        let timestamp_fields = [
            "DateTimeOriginal",
            "CreationDate",
            "MediaCreateDate", 
            "TrackCreateDate",
            "Create Date",
            "MakerNotes:CreateDate",
            "MediaModifyDate",
            "TrackModifyDate",
            "Modify Date",
            "MakerNotes:ModifyDate",
            "NikonDateTime",
            "ModifyDate",
        ];

        for field in timestamp_fields {
            if let Some(timestamp_str) = metadata.get(field) {
                // Skip file system dates that are unreliable
                if field.contains("File") && self.is_recent_timestamp(timestamp_str) {
                    continue;
                }
                
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

        // 3. Fallback to base timestamps only (avoiding file system dates)
        let fallback_fields = [
            "DateTimeOriginal",
            "CreationDate",
            "Create Date",
            "MakerNotes:CreateDate",
            "ModifyDate",
            "Modify Date", 
            "MakerNotes:ModifyDate",
            "DateTimeDigitized"
        ];

        for field in fallback_fields {
            if let Some(timestamp_str) = metadata.get(field) {
                // Skip file system dates that are unreliable
                if field.contains("File") && self.is_recent_timestamp(timestamp_str) {
                    continue;
                }
                
                if let Ok((dt, ms)) = self.parse_timestamp_with_subseconds(timestamp_str) {
                    return Ok((dt, ms));
                }
            }
        }

        anyhow::bail!("No valid timestamp found in photo EXIF data");
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
    pub fn write_timestamp(&self, file_path: &Path, timestamp: DateTime<Utc>) -> Result<()> {
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

    /// Check if a timestamp is suspiciously recent (likely a file system date)
    fn is_recent_timestamp(&self, timestamp_str: &str) -> bool {
        if let Ok((dt, _)) = self.parse_timestamp_with_subseconds(timestamp_str) {
            // If timestamp is after 2024, it's likely a file system date
            dt.year() > 2024
        } else {
            false
        }
    }
}
