#!/bin/sh

project_dir=$(CDPATH='' cd -- "$(dirname -- "$0")/.." && pwd) || exit 1
cd "$project_dir" || exit 1

for cut in launch social; do
  if [ "$cut" = launch ]; then
    dimensions=1280:720
  else
    dimensions=720:720
  fi

  ffmpeg -y -hide_banner -loglevel warning \
    -i "out/pgs-$cut.mp4" \
    -vf "scale=$dimensions:flags=lanczos" \
    -c:v libx264 -preset slow -crf 23 -profile:v main -level:v 3.1 \
    -pix_fmt yuv420p -maxrate 1200k -bufsize 2400k \
    -g 60 -keyint_min 30 -threads 4 \
    -c:a aac -b:a 96k -ar 48000 -movflags +faststart \
    "../../docs/assets/pgs-$cut.mp4" || exit 1
done
