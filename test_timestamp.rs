use anyhow::{Context, Result};
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use std::collections::HashMap;
use exif::Reader as ExifReader;
use chrono::{DateTime, NaiveDateTime, Utc};

fn parse_timestamp_with_subseconds(timestamp_str: &str) -> Result<(DateTime<Utc>, u16)> {
    let timestamp_str = timestamp_str.trim();
    
    // Handle EXIF format timestamps from kamadak-exif
    let (main_part, subsec_part) = if timestamp_str.contains('.') {
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

fn main() -> Result<()> {
    let test_file = Path::new("/projects/sortify/sortify-rs/2025/09-Sep/20250924_082049.680.jpg");
    
    println!("Testing timestamp extraction on: {}", test_file.display());
    
    let file = File::open(test_file)?;
    let mut bufreader = BufReader::new(&file);
    let exifreader = ExifReader::new();
    match exifreader.read_from_container(&mut bufreader) {
        Ok(exif) => {
            println!("kamadak-exif found EXIF data!");
            let mut metadata = HashMap::new();
            for field in exif.fields() {
                let tag_name = field.tag.to_string();
                let value = field.display_value().with_unit(&exif).to_string();
                metadata.insert(tag_name, value);
            }
            
            // Test the timestamp extraction logic
            let combinations = [
                ("DateTimeOriginal", "SubSecTimeOriginal"),
                ("ModifyDate", "SubSecTime"),
                ("DateTimeDigitized", "SubSecTimeDigitized")
            ];

            for (base_field, subsec_field) in combinations {
                if let (Some(base_time), Some(subsec_value)) = (metadata.get(base_field), metadata.get(subsec_field)) {
                    println!("Found combination: {} = {}, {} = {}", base_field, base_time, subsec_field, subsec_value);
                    
                    let padded_subsec = format!("{:0<3}", subsec_value);
                    let combined_timestamp = format!("{}.{}", base_time, padded_subsec);
                    println!("Combined timestamp: {}", combined_timestamp);
                    
                    match parse_timestamp_with_subseconds(&combined_timestamp) {
                        Ok((dt, ms)) => {
                            println!("SUCCESS: Parsed as {} with {}ms", dt, ms);
                            return Ok(());
                        }
                        Err(e) => {
                            println!("FAILED: {}", e);
                        }
                    }
                }
            }
            
            println!("No valid timestamp combination found");
        }
        Err(e) => println!("kamadak-exif parsing failed: {}", e),
    }
    
    Ok(())
}
