# EXIF Extraction Benchmark

This benchmark compares the performance of different EXIF extraction methods:

1. **exif crate (libexif)** - C library wrapper
2. **kamadak-exif** - Pure Rust implementation  
3. **fast-exif-reader** - Optimized pure Rust
4. **exiftool** - External Perl tool

## Quick Start

```bash
# Run the benchmark
./run_benchmark.sh

# Or run directly with cargo
cargo bench --bench exif_benchmark
```

## Expected Performance Ranking

Based on typical performance characteristics:

1. **fast-exif-reader** - Fastest (pure Rust, optimized)
2. **kamadak-exif** - Fast (pure Rust, standard)
3. **exif crate** - Medium (libexif, C library)
4. **exiftool** - Slowest (external process, most comprehensive)

## Dependencies

### Required
- Rust toolchain
- Test images in `test_data/` directory

### Optional (for specific methods)
- **libexif** - For exif crate benchmarks
  ```bash
  # Ubuntu/Debian
  sudo apt-get install libexif-dev
  
  # macOS
  brew install libexif
  ```

- **exiftool** - For exiftool benchmarks
  ```bash
  # Ubuntu/Debian
  sudo apt-get install exiftool
  
  # macOS
  brew install exiftool
  ```

## Benchmark Results

Results are saved in `target/criterion/` directory:
- **HTML Report**: `target/criterion/index.html`
- **Comparison**: `target/criterion/exif_comparison/report/index.html`
- **JSON Data**: `benchmark_results.json`

## Test Images

Place test images in the `test_data/` directory:
- `sample_image.jpg`
- `output_with_exif.jpg`
- `target_image.jpg`

The benchmark will automatically create a sample image if ImageMagick is available.

## Analysis

The benchmark measures:
- **Mean execution time** (nanoseconds)
- **Standard deviation**
- **Throughput** (images per second)
- **Iterations** (number of runs)

## Fallback Strategy

Based on benchmark results, the recommended fallback order is:

```
fast-exif-reader → kamadak-exif → exif crate (libexif) → exiftool
```

This provides:
- **Maximum performance** with fast-exif-reader
- **Good compatibility** with kamadak-exif
- **System library support** with exif crate
- **Maximum compatibility** with exiftool

## Troubleshooting

### libexif not found
```
error: The system library `libexif` required by crate `exif-sys` was not found.
```
**Solution**: Install libexif development package

### exiftool not found
```
error: exiftool failed: No such file or directory
```
**Solution**: Install exiftool package

### No test images
```
warning: test_data directory not found
```
**Solution**: Add test images to `test_data/` directory or install ImageMagick for automatic generation
