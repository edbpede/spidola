#!/usr/bin/env bash
# SPDX-FileCopyrightText: 2026 Spidola contributors
# SPDX-License-Identifier: AGPL-3.0-or-later

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
output_dir="${1:-${repo_root}/target/test-headend-assets}"
duration="${2:-${SPIDOLA_HEADEND_DURATION_SECONDS:-60}}"

if [[ ! "${duration}" =~ ^[1-9][0-9]*$ ]]; then
    printf 'fixture duration must be a positive integer, got: %s\n' "${duration}" >&2
    exit 1
fi

if ! command -v ffmpeg >/dev/null 2>&1; then
    printf '%s\n' 'ffmpeg is required to generate the synthetic headend fixtures' >&2
    exit 1
fi

ffmpeg_configuration="$(ffmpeg -version | sed -n 's/^configuration: //p')"
if [[ " ${ffmpeg_configuration} " == *" --enable-gpl "* ]] ||
   [[ " ${ffmpeg_configuration} " == *" --enable-nonfree "* ]]; then
    printf '%s\n' 'refusing a GPL/nonfree FFmpeg build; use the project LGPL configuration' >&2
    exit 1
fi

encoder_available() {
    ffmpeg -hide_banner -encoders 2>/dev/null | awk '{print $2}' | grep -Fxq "$1"
}

select_encoder() {
    local requested="$1"
    shift
    if [[ -n "${requested}" ]]; then
        if ! encoder_available "${requested}"; then
            printf 'requested encoder is unavailable: %s\n' "${requested}" >&2
            exit 1
        fi
        printf '%s' "${requested}"
        return
    fi
    local candidate
    for candidate in "$@"; do
        if encoder_available "${candidate}"; then
            printf '%s' "${candidate}"
            return
        fi
    done
    printf 'none of the required encoders are available: %s\n' "$*" >&2
    exit 1
}

h264_encoder="$(select_encoder "${SPIDOLA_H264_ENCODER:-}" h264_videotoolbox)"
hevc_encoder="$(select_encoder "${SPIDOLA_HEVC_ENCODER:-}" hevc_videotoolbox libkvazaar)"
vp9_encoder="$(select_encoder "${SPIDOLA_VP9_ENCODER:-}" libvpx-vp9)"

rm -rf "${output_dir}"
mkdir -p \
    "${output_dir}/hls-h264-aac" \
    "${output_dir}/hls-hevc-eac3" \
    "${output_dir}/dash-h264-aac" \
    "${output_dir}/hls-multi-audio-subs/video" \
    "${output_dir}/hls-multi-audio-subs/audio-en" \
    "${output_dir}/hls-multi-audio-subs/audio-da" \
    "${output_dir}/hls-multi-audio-subs/audio-de" \
    "${output_dir}/hls-multi-audio-subs/subs-en" \
    "${output_dir}/hls-multi-audio-subs/subs-da"

video_source="testsrc2=size=1280x720:rate=30:duration=${duration}"
audio_source="sine=frequency=440:sample_rate=48000:duration=${duration}"
common=( -hide_banner -loglevel warning -y )
h264=( -c:v "${h264_encoder}" -profile:v high -pix_fmt yuv420p -g 60 )

ffmpeg "${common[@]}" -f lavfi -i "${video_source}" -f lavfi -i "${audio_source}" \
    -shortest "${h264[@]}" -c:a aac -b:a 128k -f hls -hls_time 4 -hls_playlist_type vod \
    -hls_segment_type fmp4 -hls_fmp4_init_filename init.mp4 \
    -hls_segment_filename "${output_dir}/hls-h264-aac/segment-%03d.m4s" \
    "${output_dir}/hls-h264-aac/master.m3u8"

ffmpeg "${common[@]}" -f lavfi -i "${video_source}" -f lavfi -i "${audio_source}" \
    -shortest -c:v "${hevc_encoder}" -profile:v main10 -pix_fmt p010le -g 60 \
    -c:a eac3 -b:a 384k -f hls -hls_time 4 -hls_playlist_type vod \
    -hls_segment_type fmp4 -hls_fmp4_init_filename init.mp4 \
    -hls_segment_filename "${output_dir}/hls-hevc-eac3/segment-%03d.m4s" \
    "${output_dir}/hls-hevc-eac3/master.m3u8"

# FFmpeg expands the DASH template variables; the shell must preserve them literally.
# shellcheck disable=SC2016
ffmpeg "${common[@]}" -f lavfi -i "${video_source}" -f lavfi -i "${audio_source}" \
    -shortest "${h264[@]}" -c:a aac -b:a 128k -f dash -seg_duration 4 \
    -init_seg_name 'init-$RepresentationID$.m4s' \
    -media_seg_name 'segment-$RepresentationID$-$Number%05d$.m4s' \
    "${output_dir}/dash-h264-aac/manifest.mpd"

ffmpeg "${common[@]}" -f lavfi -i "${video_source}" -f lavfi -i "${audio_source}" \
    -shortest -c:v mpeg2video -g 30 -c:a mp2 -b:a 192k -f mpegts \
    "${output_dir}/ts-mpeg2-mp2.ts"

ffmpeg "${common[@]}" -f lavfi -i "${video_source}" -f lavfi -i "${audio_source}" \
    -shortest "${h264[@]}" -c:a aac -b:a 128k \
    -streamid 0:0x100 -streamid 1:0x101 -f mpegts "${output_dir}/ts-h264-aac.ts"

ffmpeg "${common[@]}" -f lavfi -i "${video_source}" -f lavfi -i "${audio_source}" \
    -shortest -c:v "${vp9_encoder}" -deadline good -cpu-used 4 -b:v 2M -c:a libopus -b:a 128k \
    "${output_dir}/mkv-vp9-opus.mkv"

ffmpeg "${common[@]}" -f lavfi -i "${video_source}" "${h264[@]}" -an \
    -f hls -hls_time 4 -hls_playlist_type vod \
    -hls_segment_filename "${output_dir}/hls-multi-audio-subs/video/segment-%03d.ts" \
    "${output_dir}/hls-multi-audio-subs/video/index.m3u8"

generate_audio() {
    local frequency="$1"
    local directory="$2"
    ffmpeg "${common[@]}" -f lavfi \
        -i "sine=frequency=${frequency}:sample_rate=48000:duration=${duration}" \
        -vn -c:a aac -b:a 128k -f hls -hls_time 4 -hls_playlist_type vod \
        -hls_segment_filename "${directory}/segment-%03d.ts" "${directory}/index.m3u8"
}

generate_audio 440 "${output_dir}/hls-multi-audio-subs/audio-en"
generate_audio 554 "${output_dir}/hls-multi-audio-subs/audio-da"
generate_audio 659 "${output_dir}/hls-multi-audio-subs/audio-de"

write_subtitles() {
    local language="$1"
    local label="$2"
    local directory="${output_dir}/hls-multi-audio-subs/subs-${language}"
    printf 'WEBVTT\n\n00:00:00.000 --> 00:00:10.000\n%s synthetic subtitle\n' "${label}" > "${directory}/captions.vtt"
    printf '#EXTM3U\n#EXT-X-VERSION:3\n#EXT-X-TARGETDURATION:%s\n#EXT-X-MEDIA-SEQUENCE:0\n#EXTINF:%s.000,\ncaptions.vtt\n#EXT-X-ENDLIST\n' \
        "${duration}" "${duration}" > "${directory}/index.m3u8"
}

write_subtitles en English
write_subtitles da Danish

printf '%s\n' \
    '#EXTM3U' \
    '#EXT-X-VERSION:6' \
    '#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="English",LANGUAGE="en",DEFAULT=YES,AUTOSELECT=YES,URI="audio-en/index.m3u8"' \
    '#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="Danish",LANGUAGE="da",DEFAULT=NO,AUTOSELECT=YES,URI="audio-da/index.m3u8"' \
    '#EXT-X-MEDIA:TYPE=AUDIO,GROUP-ID="audio",NAME="German",LANGUAGE="de",DEFAULT=NO,AUTOSELECT=YES,URI="audio-de/index.m3u8"' \
    '#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="English",LANGUAGE="en",DEFAULT=NO,AUTOSELECT=YES,FORCED=NO,URI="subs-en/index.m3u8"' \
    '#EXT-X-MEDIA:TYPE=SUBTITLES,GROUP-ID="subs",NAME="Danish",LANGUAGE="da",DEFAULT=NO,AUTOSELECT=YES,FORCED=NO,URI="subs-da/index.m3u8"' \
    '#EXT-X-STREAM-INF:BANDWIDTH=3500000,RESOLUTION=1280x720,CODECS="avc1.64001f,mp4a.40.2",AUDIO="audio",SUBTITLES="subs"' \
    'video/index.m3u8' > "${output_dir}/hls-multi-audio-subs/master.m3u8"

printf 'generated synthetic headend assets in %s\n' "${output_dir}"
