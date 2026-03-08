#!/bin/bash
# Test script for seamless looping

echo "Testing seamless looping with HAP video..."
echo "Usage: ./test_loop.sh <path_to_hap_video>"

VIDEO_FILE="${1:-samples/test.mov}"

if [ ! -f "$VIDEO_FILE" ]; then
    echo "Error: Video file not found: $VIDEO_FILE"
    echo "Please provide a HAP video file path"
    exit 1
fi

echo ""
echo "Testing with: $VIDEO_FILE"
echo ""
echo "Controls:"
echo "  - Click pad 1 to trigger playback"
echo "  - Right-click pad 1 -> Settings to enable Loop"
echo "  - Watch for smooth loop transition at end"
echo ""
echo "Press Ctrl+C to exit"
echo ""

cargo run --release -- --simple --file "$VIDEO_FILE" --loop-playback 2>&1 | grep -E "(Starting stream|loop|Loop|frame)"
