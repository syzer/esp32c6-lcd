release:
  cargo run --release

resizeImages:
    #!/usr/bin/env bash
    set -euo pipefail
    shopt -s nullglob

    mkdir -p assets/rgb

    files=(assets/png/*.{png,PNG,jpg,JPG,jpeg,JPEG})
    if [[ ${#files[@]} -eq 0 ]]; then
        echo "No images found in assets/png/ (looking for .png/.jpg/.jpeg)."
        exit 1
    fi

    if ! command -v ffmpeg >/dev/null 2>&1; then
      echo "ffmpeg not found. Please install it (brew install ffmpeg)" >&2
      exit 1
    fi

    idx=1
    for f in "${files[@]}"; do
        base="$(basename "$f")"
        out="assets/rgb/pic_${idx}_172x320.rgb565"
        echo "Converting $base -> ${out}"
        ffmpeg -v error -y -i "$f" -vf scale=172:320,format=rgb565le -f rawvideo "$out"
        if [[ -f "$out" ]]; then
            echo "  ✓ $(du -h "$out" | cut -f1)  $out"
        else
            echo "  ✗ Failed to create $out" >&2
            exit 1
        fi
        idx=$((idx+1))
    done

    echo "✅ Done. Converted ${#files[@]} image(s) to assets/rgb/"