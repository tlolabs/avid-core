#!/bin/sh
if [ "$1" = '-version' ]; then echo 'ffmpeg version test'; exit; fi
if [ "$2" = '-encoders' ]; then printf ' V..... libx264 software\n V..... h264_videotoolbox hardware\n'; exit; fi
# Mode is ordinary data, separate from the immutable executable inode.
mode=$(cat "${0%/*}/fixture-mode")
encoder=''
previous=''
for arg in "$@"; do
  if [ "$previous" = '-c:v' ]; then encoder="$arg"; fi
  previous="$arg"
  output="$arg"
done
printf partial > "$output"
printf 'out_time_us=1000000\nspeed=2x\nprogress=continue\n'
if [ "$mode" = 'slow' ]; then exec sleep 10; fi
if [ "$mode" = 'fail' ] || [ "$encoder" = 'h264_videotoolbox' ]; then echo 'deliberate encoder failure' >&2; exit 7; fi
printf completed > "$output"
printf 'out_time_us=2000000\nspeed=2x\nprogress=end\n'
