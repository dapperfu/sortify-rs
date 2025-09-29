use criterion::{black_box, criterion_group, criterion_main, Criterion, BenchmarkId};
use rayon::prelude::*;
use walkdir::WalkDir;

// Import our EXIF processing modules from the local crate
use sortify_rs::exif::{ExifProcessor, ExifData};

/// Load real image files from the test directory
fn load_real_test_images() -> Vec<std::path::PathBuf> {
    let test_dir = "/keg/pictures/incoming/2025.old/09-Sep";
    let mut test_files = Vec::new();
    
    println!("Loading real image files from: {}", test_dir);
    
    // Walk through the directory and collect image files
    for entry in WalkDir::new(test_dir)
        .into_iter()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_file())
    {
        let path = entry.path();
        if let Some(ext) = path.extension() {
            let ext_str = ext.to_string_lossy().to_lowercase();
            if matches!(ext_str.as_str(), "jpg" | "jpeg" | "hif" | "heic" | "png" | "tiff" | "tif") {
                test_files.push(path.to_path_buf());
                
                // Limit to 100 files for reasonable benchmark time
                if test_files.len() >= 100 {
                    break;
                }
            }
        }
    }
    
    println!("Loaded {} real image files for benchmarking", test_files.len());
    test_files
}

/// Benchmark nom-exif with async processing (simplified to blocking for now)
fn benchmark_nom_exif_async(files: &[std::path::PathBuf]) -> Vec<Result<ExifData, anyhow::Error>> {
    let processor = ExifProcessor::new();
    
    // Process files concurrently using rayon
    files.par_iter().map(|file| {
        processor.extract_exif_data_nom_exif_blocking(file)
    }).collect()
}

/// Benchmark kamadak-exif with parallel processing
fn benchmark_kamadak_exif_parallel(files: &[std::path::PathBuf]) -> Vec<Result<ExifData, anyhow::Error>> {
    let processor = ExifProcessor::new();
    
    // Process files in parallel using rayon
    files.par_iter().map(|file| {
        processor.extract_exif_data_kamadak(file)
    }).collect()
}


/// Benchmark nom-exif with sequential processing
fn benchmark_nom_exif_sequential(files: &[std::path::PathBuf]) -> Vec<Result<ExifData, anyhow::Error>> {
    let processor = ExifProcessor::new();
    
    files.iter().map(|file| {
        // Use blocking version for sequential processing
        processor.extract_exif_data_nom_exif_blocking(file)
    }).collect()
}

/// Benchmark kamadak-exif with sequential processing
fn benchmark_kamadak_exif_sequential(files: &[std::path::PathBuf]) -> Vec<Result<ExifData, anyhow::Error>> {
    let processor = ExifProcessor::new();
    
    files.iter().map(|file| {
        processor.extract_exif_data_kamadak(file)
    }).collect()
}

fn benchmark_exif_libraries(c: &mut Criterion) {
    let test_files = load_real_test_images();
    
    let mut group = c.benchmark_group("EXIF Processing Performance");
    group.sample_size(10); // Minimum sample size for criterion
    
    // Test different file counts to see scaling behavior
    for file_count in [1, 5, 10].iter() {
        if *file_count > test_files.len() {
            continue;
        }
        let files = &test_files[..*file_count];
        
        // Benchmark nom-exif parallel
        group.bench_with_input(
            BenchmarkId::new("nom-exif-parallel", file_count),
            file_count,
            |b, _| {
                b.iter(|| {
                    benchmark_nom_exif_async(black_box(files))
                })
            },
        );
        
        // Benchmark nom-exif sequential
        group.bench_with_input(
            BenchmarkId::new("nom-exif-sequential", file_count),
            file_count,
            |b, _| {
                b.iter(|| {
                    benchmark_nom_exif_sequential(black_box(files))
                })
            },
        );
        
        // Benchmark kamadak-exif parallel
        group.bench_with_input(
            BenchmarkId::new("kamadak-exif-parallel", file_count),
            file_count,
            |b, _| {
                b.iter(|| {
                    benchmark_kamadak_exif_parallel(black_box(files))
                })
            },
        );
        
        // Benchmark kamadak-exif sequential
        group.bench_with_input(
            BenchmarkId::new("kamadak-exif-sequential", file_count),
            file_count,
            |b, _| {
                b.iter(|| {
                    benchmark_kamadak_exif_sequential(black_box(files))
                })
            },
        );
        
    }
    
    group.finish();
}

/// Benchmark memory usage and throughput
fn benchmark_throughput(c: &mut Criterion) {
    let test_files = load_real_test_images();
    
    let mut group = c.benchmark_group("EXIF Throughput");
    group.throughput(criterion::Throughput::Elements(test_files.len() as u64));
    
    // Test throughput with all files
    group.bench_function("nom-exif-parallel-throughput", |b| {
        b.iter(|| {
            benchmark_nom_exif_async(black_box(&test_files))
        })
    });
    
    group.bench_function("kamadak-exif-parallel-throughput", |b| {
        b.iter(|| {
            benchmark_kamadak_exif_parallel(black_box(&test_files))
        })
    });
    
    
    group.finish();
}

/// Benchmark error handling and fallback behavior
fn benchmark_error_handling(c: &mut Criterion) {
    // Use a mix of real files and some that might cause errors
    let mut test_files = load_real_test_images();
    
    // Add some problematic files for error testing
    let invalid_file = std::path::PathBuf::from("/tmp/invalid.jpg");
    std::fs::write(&invalid_file, b"not_a_valid_image").unwrap();
    
    let empty_file = std::path::PathBuf::from("/tmp/empty.jpg");
    std::fs::write(&empty_file, b"").unwrap();
    
    test_files.push(invalid_file);
    test_files.push(empty_file);
    
    let mut group = c.benchmark_group("EXIF Error Handling");
    
    group.bench_function("nom-exif-error-handling", |b| {
        b.iter(|| {
            benchmark_nom_exif_async(black_box(&test_files))
        })
    });
    
    group.bench_function("kamadak-exif-error-handling", |b| {
        b.iter(|| {
            benchmark_kamadak_exif_parallel(black_box(&test_files))
        })
    });
    
    group.finish();
}

criterion_group!(
    benches,
    benchmark_exif_libraries,
    benchmark_throughput,
    benchmark_error_handling
);
criterion_main!(benches);
