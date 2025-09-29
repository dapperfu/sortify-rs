use anyhow::Result;
use std::path::Path;
use std::fs::File;
use std::io::BufReader;
use std::collections::HashMap;
use exif::Reader as ExifReader;

fn main() -> Result<()> {
    let test_file = Path::new("/keg/pictures/incoming/2025.old/09-Sep/20250928_151944.600.jpg");
    
    println!("Testing kamadak-exif field extraction on: {}", test_file.display());
    
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
            
            println!("\nAll timestamp-related fields found by kamadak-exif:");
            for (key, value) in &metadata {
                if key.to_lowercase().contains("time") || 
                   key.to_lowercase().contains("date") ||
                   key.to_lowercase().contains("gps") ||
                   key.to_lowercase().contains("subsec") {
                    println!("  {}: {}", key, value);
                }
            }
            
            println!("\nTotal fields found: {}", metadata.len());
        }
        Err(e) => println!("kamadak-exif parsing failed: {}", e),
    }
    
    Ok(())
}
