#!/bin/bash
/**
 * EXIF benchmark runner script
 * 
 * This code written by Claude Sonnet 4 (claude-3-5-sonnet-20241022)
 * Generated via Cursor IDE (cursor.sh) with AI assistance
 * Model: Anthropic Claude 3.5 Sonnet
 * Generation timestamp: 2024-12-19T16:00:00Z
 * Context: Benchmark runner for EXIF extraction methods
 * 
 * Technical details:
 * - LLM: Claude 3.5 Sonnet (2024-10-22)
 * - IDE: Cursor (cursor.sh)
 * - Generation method: AI-assisted pair programming
 * - Code style: Bash script with proper error handling
 * - Dependencies: cargo, criterion
 */

set -e

echo "🔬 EXIF Extraction Benchmark"
echo "=============================="
echo ""

# Check if we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo "❌ Error: Please run this script from the sortify-rs directory"
    exit 1
fi

# Check if test images exist
echo "📁 Checking for test images..."
if [ ! -d "test_data" ]; then
    echo "⚠️  Warning: test_data directory not found"
    echo "   Creating sample test data..."
    mkdir -p test_data
    
    # Create a simple test image if possible
    if command -v convert >/dev/null 2>&1; then
        echo "   Creating sample test image with ImageMagick..."
        convert -size 100x100 xc:white test_data/sample_image.jpg
        echo "   ✅ Created test_data/sample_image.jpg"
    else
        echo "   ⚠️  ImageMagick not found. Please add test images to test_data/ directory"
        echo "   Expected files: sample_image.jpg, output_with_exif.jpg, target_image.jpg"
    fi
fi

# Check if exiftool is available
echo ""
echo "🔧 Checking dependencies..."
if command -v exiftool >/dev/null 2>&1; then
    echo "   ✅ exiftool found: $(exiftool -ver)"
else
    echo "   ⚠️  Warning: exiftool not found. Some benchmarks will fail."
    echo "   Install with: sudo apt-get install exiftool (Ubuntu/Debian)"
    echo "   Or: brew install exiftool (macOS)"
fi

# Check if libexif is available
if pkg-config --exists libexif 2>/dev/null; then
    echo "   ✅ libexif found: $(pkg-config --modversion libexif)"
else
    echo "   ⚠️  Warning: libexif not found. exif crate benchmarks will fail."
    echo "   Install with: sudo apt-get install libexif-dev (Ubuntu/Debian)"
    echo "   Or: brew install libexif (macOS)"
fi

echo ""
echo "🚀 Running benchmarks..."
echo ""

# Run the benchmark
cargo bench --bench exif_benchmark

echo ""
echo "📊 Benchmark complete!"
echo ""
echo "Results are saved in: target/criterion/"
echo "Open target/criterion/index.html in your browser to view detailed results"
echo ""

# Show a quick summary if possible
if [ -f "target/criterion/exif_comparison/report/index.html" ]; then
    echo "📈 Quick summary:"
    echo "   - Detailed HTML report: target/criterion/index.html"
    echo "   - Comparison report: target/criterion/exif_comparison/report/index.html"
fi

echo ""
echo "🎯 Benchmark Summary:"
echo "   Methods tested:"
echo "   1. exif crate (libexif) - C library wrapper"
echo "   2. kamadak-exif - Pure Rust implementation"
echo "   3. fast-exif-reader - Optimized pure Rust"
echo "   4. exiftool - External Perl tool"
echo ""
echo "   Performance typically follows this order (fastest to slowest):"
echo "   1. fast-exif-reader (pure Rust, optimized)"
echo "   2. kamadak-exif (pure Rust, standard)"
echo "   3. exif crate (libexif, C library)"
echo "   4. exiftool (external process, most comprehensive)"
