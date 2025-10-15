/**
 * EXIF Writer module - Custom implementation for writing EXIF data
 * 
 * Based on EXIF specification and exiftool algorithms:
 * - EXIF 2.3 specification compliance
 * - Binary format handling for JPEG and TIFF files
 * - Tag structure and IFD (Image File Directory) management
 * - Endianness handling (Little-endian/Big-endian)
 */

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use log::debug;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

/// EXIF tag types as defined in EXIF specification
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExifTagType {
    Byte = 1,
    Ascii = 2,
    Short = 3,
    Long = 4,
    Rational = 5,
    Undefined = 7,
    SLong = 9,
    SRational = 10,
}

impl ExifTagType {
    pub fn size(&self) -> u32 {
        match self {
            ExifTagType::Byte => 1,
            ExifTagType::Ascii => 1,
            ExifTagType::Short => 2,
            ExifTagType::Long => 4,
            ExifTagType::Rational => 8,
            ExifTagType::Undefined => 1,
            ExifTagType::SLong => 4,
            ExifTagType::SRational => 8,
        }
    }
}

/// EXIF tag definition
#[derive(Debug, Clone)]
pub struct ExifTag {
    pub tag_id: u16,
    pub tag_type: ExifTagType,
    pub count: u32,
    pub value: Vec<u8>,
}

/// EXIF IFD (Image File Directory) structure
#[derive(Debug, Clone)]
pub struct ExifIfd {
    pub entries: Vec<ExifTag>,
    pub next_ifd_offset: u32,
}

/// EXIF writer for creating and modifying EXIF data
pub struct ExifWriter {
    primary_ifd: ExifIfd,
    _exif_ifd: Option<ExifIfd>,
    _thumbnail_ifd: Option<ExifIfd>,
    is_little_endian: bool,
}

impl ExifWriter {
    pub fn new() -> Self {
        Self {
            primary_ifd: ExifIfd {
                entries: Vec::new(),
                next_ifd_offset: 0,
            },
            _exif_ifd: None,
            _thumbnail_ifd: None,
            is_little_endian: true, // Default to little-endian
        }
    }

    /// Add a timestamp tag to the EXIF data
    pub fn add_timestamp(&mut self, tag_name: &str, timestamp: DateTime<Utc>) -> Result<()> {
        let formatted_time = timestamp.format("%Y:%m:%d %H:%M:%S").to_string();
        self.add_ascii_tag(tag_name, &formatted_time)?;
        Ok(())
    }

    /// Add an ASCII string tag
    pub fn add_ascii_tag(&mut self, tag_name: &str, value: &str) -> Result<()> {
        let tag_id = self.get_tag_id(tag_name)?;
        let mut ascii_bytes = value.as_bytes().to_vec();
        ascii_bytes.push(0); // Null terminator for ASCII strings
        
        let tag = ExifTag {
            tag_id,
            tag_type: ExifTagType::Ascii,
            count: ascii_bytes.len() as u32,
            value: ascii_bytes,
        };
        
        self.primary_ifd.entries.push(tag);
        Ok(())
    }

    /// Add a short (16-bit) integer tag
    pub fn add_short_tag(&mut self, tag_name: &str, value: u16) -> Result<()> {
        let tag_id = self.get_tag_id(tag_name)?;
        let bytes = if self.is_little_endian {
            value.to_le_bytes().to_vec()
        } else {
            value.to_be_bytes().to_vec()
        };
        
        let tag = ExifTag {
            tag_id,
            tag_type: ExifTagType::Short,
            count: 1,
            value: bytes,
        };
        
        self.primary_ifd.entries.push(tag);
        Ok(())
    }

    /// Add a long (32-bit) integer tag
    pub fn add_long_tag(&mut self, tag_name: &str, value: u32) -> Result<()> {
        let tag_id = self.get_tag_id(tag_name)?;
        let bytes = if self.is_little_endian {
            value.to_le_bytes().to_vec()
        } else {
            value.to_be_bytes().to_vec()
        };
        
        let tag = ExifTag {
            tag_id,
            tag_type: ExifTagType::Long,
            count: 1,
            value: bytes,
        };
        
        self.primary_ifd.entries.push(tag);
        Ok(())
    }

    /// Convert tag name to tag ID (EXIF specification mapping)
    fn get_tag_id(&self, tag_name: &str) -> Result<u16> {
        let tag_map: HashMap<&str, u16> = [
            // Primary IFD tags
            ("ImageWidth", 0x0100),
            ("ImageLength", 0x0101),
            ("BitsPerSample", 0x0102),
            ("Compression", 0x0103),
            ("PhotometricInterpretation", 0x0106),
            ("Orientation", 0x0112),
            ("SamplesPerPixel", 0x0115),
            ("PlanarConfiguration", 0x011C),
            ("YCbCrSubSampling", 0x0212),
            ("YCbCrPositioning", 0x0213),
            ("XResolution", 0x011A),
            ("YResolution", 0x011B),
            ("ResolutionUnit", 0x0128),
            ("DateTime", 0x0132),
            ("Artist", 0x013B),
            ("Copyright", 0x8298),
            
            // EXIF IFD tags
            ("ExposureTime", 0x829A),
            ("FNumber", 0x829D),
            ("ExposureProgram", 0x8822),
            ("ISOSpeedRatings", 0x8827),
            ("ExifVersion", 0x9000),
            ("DateTimeOriginal", 0x9003),
            ("DateTimeDigitized", 0x9004),
            ("ComponentsConfiguration", 0x9101),
            ("CompressedBitsPerPixel", 0x9102),
            ("ShutterSpeedValue", 0x9201),
            ("ApertureValue", 0x9202),
            ("BrightnessValue", 0x9203),
            ("ExposureBiasValue", 0x9204),
            ("MaxApertureValue", 0x9205),
            ("SubjectDistance", 0x9206),
            ("MeteringMode", 0x9207),
            ("LightSource", 0x9208),
            ("Flash", 0x9209),
            ("FocalLength", 0x920A),
            ("SubjectArea", 0x9214),
            ("MakerNote", 0x927C),
            ("UserComment", 0x9286),
            ("SubSecTime", 0x9290),
            ("SubSecTimeOriginal", 0x9291),
            ("SubSecTimeDigitized", 0x9292),
            ("FlashpixVersion", 0xA000),
            ("ColorSpace", 0xA001),
            ("PixelXDimension", 0xA002),
            ("PixelYDimension", 0xA003),
            ("RelatedSoundFile", 0xA004),
            ("InteroperabilityIFD", 0xA005),
            ("FlashEnergy", 0xA20B),
            ("SpatialFrequencyResponse", 0xA20C),
            ("FocalPlaneXResolution", 0xA20E),
            ("FocalPlaneYResolution", 0xA20F),
            ("FocalPlaneResolutionUnit", 0xA210),
            ("SubjectLocation", 0xA214),
            ("ExposureIndex", 0xA215),
            ("SensingMethod", 0xA217),
            ("FileSource", 0xA300),
            ("SceneType", 0xA301),
            ("CFAPattern", 0xA302),
            ("CustomRendered", 0xA401),
            ("ExposureMode", 0xA402),
            ("WhiteBalance", 0xA403),
            ("DigitalZoomRatio", 0xA404),
            ("FocalLengthIn35mmFilm", 0xA405),
            ("SceneCaptureType", 0xA406),
            ("GainControl", 0xA407),
            ("Contrast", 0xA408),
            ("Saturation", 0xA409),
            ("Sharpness", 0xA40A),
            ("DeviceSettingDescription", 0xA40B),
            ("SubjectDistanceRange", 0xA40C),
            ("ImageUniqueID", 0xA420),
            ("CameraOwnerName", 0xA430),
            ("BodySerialNumber", 0xA431),
            ("LensSpecification", 0xA432),
            ("LensMake", 0xA433),
            ("LensModel", 0xA434),
            ("LensSerialNumber", 0xA435),
        ].iter().cloned().collect();
        
        tag_map.get(tag_name)
            .copied()
            .ok_or_else(|| anyhow::anyhow!("Unknown EXIF tag: {}", tag_name))
    }

    /// Write EXIF data to a JPEG file
    pub fn write_to_jpeg(&self, file_path: &Path) -> Result<()> {
        debug!("Writing EXIF data to JPEG file: {}", file_path.display());
        
        // Read the JPEG file
        let mut file_data = std::fs::read(file_path)
            .context("Failed to read JPEG file")?;
        
        // Generate EXIF data
        let exif_data = self.to_bytes()?;
        
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

    /// Write EXIF data to a TIFF file
    pub fn write_to_tiff(&self, file_path: &Path) -> Result<()> {
        debug!("Writing EXIF data to TIFF file: {}", file_path.display());
        
        // Generate complete TIFF file with EXIF data
        let tiff_data = self.to_bytes()?;
        
        // Write TIFF data to file
        std::fs::write(file_path, &tiff_data)
            .context("Failed to write TIFF file")?;
        
        debug!("Successfully wrote EXIF data to TIFF file");
        Ok(())
    }

    /// Get the binary representation of EXIF data
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut data = Vec::new();
        
        // Write TIFF header
        self.write_tiff_header(&mut data)?;
        
        // Write primary IFD
        self.write_ifd(&mut data, &self.primary_ifd)?;
        
        // TODO: Write EXIF IFD and thumbnail IFD if present
        
        Ok(data)
    }

    /// Create APP1 segment for JPEG with EXIF data
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

    /// Insert or replace APP1 segment in JPEG data
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

    /// Write TIFF header (8 bytes)
    fn write_tiff_header(&self, data: &mut Vec<u8>) -> Result<()> {
        // Byte order indicator
        if self.is_little_endian {
            data.write_all(b"II")?; // Little-endian
        } else {
            data.write_all(b"MM")?; // Big-endian
        }
        
        // TIFF magic number (42)
        let magic = if self.is_little_endian { 42u16.to_le_bytes() } else { 42u16.to_be_bytes() };
        data.write_all(&magic)?;
        
        // Offset to first IFD (will be updated later)
        let offset = if self.is_little_endian { 8u32.to_le_bytes() } else { 8u32.to_be_bytes() };
        data.write_all(&offset)?;
        
        Ok(())
    }

    /// Write IFD (Image File Directory) structure
    fn write_ifd(&self, data: &mut Vec<u8>, ifd: &ExifIfd) -> Result<()> {
        let _ifd_start = data.len();
        
        // Write number of directory entries
        let count = ifd.entries.len() as u16;
        let count_bytes = if self.is_little_endian { count.to_le_bytes() } else { count.to_be_bytes() };
        data.write_all(&count_bytes)?;
        
        // Write directory entries
        for entry in &ifd.entries {
            self.write_ifd_entry(data, entry)?;
        }
        
        // Write next IFD offset
        let next_offset_bytes = if self.is_little_endian { 
            ifd.next_ifd_offset.to_le_bytes() 
        } else { 
            ifd.next_ifd_offset.to_be_bytes() 
        };
        data.write_all(&next_offset_bytes)?;
        
        // Write tag data (if any tags have data > 4 bytes)
        for entry in &ifd.entries {
            if entry.value.len() > 4 {
                // Align to 2-byte boundary
                while data.len() % 2 != 0 {
                    data.push(0);
                }
                data.write_all(&entry.value)?;
            }
        }
        
        Ok(())
    }

    /// Write a single IFD entry (12 bytes)
    fn write_ifd_entry(&self, data: &mut Vec<u8>, entry: &ExifTag) -> Result<()> {
        // Tag ID (2 bytes)
        let tag_bytes = if self.is_little_endian { 
            entry.tag_id.to_le_bytes() 
        } else { 
            entry.tag_id.to_be_bytes() 
        };
        data.write_all(&tag_bytes)?;
        
        // Tag type (2 bytes)
        let type_bytes = if self.is_little_endian { 
            (entry.tag_type as u16).to_le_bytes() 
        } else { 
            (entry.tag_type as u16).to_be_bytes() 
        };
        data.write_all(&type_bytes)?;
        
        // Count (4 bytes)
        let count_bytes = if self.is_little_endian { 
            entry.count.to_le_bytes() 
        } else { 
            entry.count.to_be_bytes() 
        };
        data.write_all(&count_bytes)?;
        
        // Value or offset (4 bytes)
        if entry.value.len() <= 4 {
            // Value fits in 4 bytes, write directly
            let mut value_bytes = entry.value.clone();
            while value_bytes.len() < 4 {
                value_bytes.push(0);
            }
            data.write_all(&value_bytes[..4])?;
        } else {
            // Value > 4 bytes, write offset (will be updated later)
            let offset_bytes = if self.is_little_endian { 
                0u32.to_le_bytes() 
            } else { 
                0u32.to_be_bytes() 
            };
            data.write_all(&offset_bytes)?;
        }
        
        Ok(())
    }
}

impl Default for ExifWriter {
    fn default() -> Self {
        Self::new()
    }
}
