use anyhow::Result;
use std::path::Path;
use std::fs::File;
use std::collections::HashMap;
use nom_exif::parse_exif;

fn main() -> Result<()> {
    let test_file = Path::new("/keg/pictures/incoming/2025.old/09-Sep/20250928_151944.600.jpg");
    
    println!("Testing nom-exif field extraction on: {}", test_file.display());
    
    let file = File::open(test_file)?;
    match parse_exif(file, None) {
        Ok(Some(iter)) => {
            println!("nom-exif found EXIF data!");
            let mut metadata = HashMap::new();
            for entry in iter {
                if let Some(tag) = entry.tag() {
                    let tag_name = format!("{:?}", tag);
                    let value = match entry.take_result() {
                        Ok(result) => result.to_string(),
                        Err(_) => "Error".to_string(),
                    };
                    metadata.insert(tag_name, value);
                }
            }
            
            println!("\nAll timestamp-related fields found by nom-exif:");
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
        Ok(None) => println!("nom-exif found no EXIF data"),
        Err(e) => println!("nom-exif parsing failed: {}", e),
    }
    
    Ok(())
}
