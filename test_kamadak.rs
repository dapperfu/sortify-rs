use anyhow::Result;
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use std::collections::HashMap;
use exif::Reader as ExifReader;

fn main() -> Result<()> {
    let test_file = Path::new("/projects/sortify/sortify-rs/2025/09-Sep/20250924_082049.680.jpg");
    
    println!("Testing kamadak-exif on: {}", test_file.display());
    
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
            
            println!("Found {} EXIF fields", metadata.len());
            
            // Check for timestamp fields
            let timestamp_fields = ["DateTimeOriginal", "ModifyDate", "CreateDate", "DateTimeDigitized", "SubSecTimeOriginal", "SubSecTime", "SubSecTimeDigitized"];
            for field in timestamp_fields {
                if let Some(value) = metadata.get(field) {
                    println!("  {}: {}", field, value);
                }
            }
        }
        Err(e) => println!("kamadak-exif parsing failed: {}", e),
    }
    
    Ok(())
}
