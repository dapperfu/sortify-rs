pub mod exif;
pub mod exif_writer;
pub mod file_ops;
pub mod naming;
pub mod hashing;

#[cfg(test)]
mod debug_tests {
    use fast_exif_reader::FastExifReader;

    #[test]
    fn test_fast_exif_on_mp4() {
        let file_path = "/home/jed/Desktop/DCIM/KidsCamera/Camera/20240817_215548.mp4";
        
        println!("Testing fast-exif-rs on: {}", file_path);
        
        let mut fast_reader = FastExifReader::new();
        let file_path_str = file_path.to_string();
        
        match fast_reader.read_file(&file_path_str) {
            Ok(metadata) => {
                println!("✅ fast-exif-rs succeeded!");
                println!("Found {} metadata fields:", metadata.len());
                
                // Check for video-specific fields
                let video_fields = ["MediaCreateDate", "MediaModifyDate", "TrackCreateDate", "TrackModifyDate", "CreateDate", "ModifyDate"];
                println!("\nVideo-specific fields found:");
                for field in &video_fields {
                    if let Some(value) = metadata.get(*field) {
                        println!("  {}: {}", field, value);
                    }
                }
                
                // Test timestamp parsing
                println!("\nTesting timestamp parsing:");
                let exif_processor = crate::exif::ExifProcessor::new();
                for field in &video_fields {
                    if let Some(timestamp_str) = metadata.get(*field) {
                        println!("  Testing {}: '{}'", field, timestamp_str);
                        match exif_processor.parse_timestamp_with_subseconds(timestamp_str) {
                            Ok((dt, ms)) => {
                                println!("    ✅ Parsed successfully: {} ({}ms)", dt, ms);
                            }
                            Err(e) => {
                                println!("    ❌ Failed to parse: {}", e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                println!("❌ fast-exif-rs failed: {}", e);
                panic!("fast-exif-rs failed: {}", e);
            }
        }
    }
}
