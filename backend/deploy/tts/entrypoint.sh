#!/usr/bin/env bash
# Downloads the model into the /models volume on first start, then serves it.
#
# Environment (all optional):
#   TTS_MODEL_DIR            where the finished model lives      (default /models/qwen3-tts-1.7b)
#   TTS_THREADS              engine threads                      (default 8: faster than 16 here, ADR 0013)
#   TTS_MAX_REQUEST_SECONDS  per-request audio cap               (default 300; the engine's own
#                            default of 60 s silently truncates long paragraphs)
#   TTS_NICE                 niceness, so the engine yields CPU to the backend and the rest of the host (default 19)
set -euo pipefail

MODEL_DIR="${TTS_MODEL_DIR:-/models/qwen3-tts-1.7b}"
STAGING="$MODEL_DIR.partial"
THREADS="${TTS_THREADS:-8}"
MAX_SECONDS="${TTS_MAX_REQUEST_SECONDS:-300}"
NICE="${TTS_NICE:-19}"

HF_MODEL="Qwen/Qwen3-TTS-12Hz-1.7B-CustomVoice"
# Set by the Dockerfile, which also makes download_model.sh fetch exactly this revision.
# A model already in $MODEL_DIR is never downloaded again, whatever its revision.
HF_REVISION="${TTS_MODEL_REVISION:?TTS_MODEL_REVISION is not set (the Dockerfile sets it)}"

# Checks every file in $STAGING against the sizes in the Hugging Face tree listing (LFS files
# included: "size" is the real size). Deletes files of the wrong size. Returns non-zero when
# anything is missing, wrong or the listing is unreachable. Only needs curl, sed and stat.
verify_staging() {
  local tree path size actual ok=0 count=0
  tree=$(curl -fsSL --retry 3 "https://huggingface.co/api/models/$HF_MODEL/tree/$HF_REVISION?recursive=1") || return 1
  # One JSON object per line, whatever the API's own layout.
  while IFS=$'\t' read -r path size || [ -n "$path" ]; do
    [ -n "$path" ] || continue
    [ -e "$STAGING/$path" ] || continue
    count=$((count + 1))
    actual=$(stat -c %s "$STAGING/$path")
    if [ "$actual" != "$size" ]; then
      echo "bad size for $path: $actual, expected $size: deleting" >&2
      rm -f "$STAGING/$path"
      ok=1
    fi
  done < <(printf '%s' "$tree" | sed 's/},{/}\n{/g' \
    | sed -n 's/.*"type":"file".*"size":\([0-9]*\).*"path":"\([^"]*\)".*/\2\t\1/p')
  [ "$ok" = 0 ] && [ "$count" -ge 11 ] \
    && [ -f "$STAGING/model.safetensors" ] && [ -f "$STAGING/speech_tokenizer/model.safetensors" ]
}

# A model directory exists only after the download finished: it is moved into place
# (atomic rename, same volume) as the very last step. Partial files stay in $STAGING.
if [ ! -f "$MODEL_DIR/model.safetensors" ] || [ ! -f "$MODEL_DIR/speech_tokenizer/model.safetensors" ]; then
  echo "model not found in $MODEL_DIR: downloading (about 4.5 GB, resumable)"
  rm -rf "$MODEL_DIR"
  until /opt/qwen3-tts/download_model.sh --model large --dir "$STAGING"; do
    echo "download failed, retrying in 30 s (partial files are kept and resumed)" >&2
    sleep 30
  done
  # download_model.sh promotes a file only when curl succeeded, but it trusts any file it finds
  # and never checks sizes: compare each file with the size Hugging Face publishes, and delete
  # a mismatch so the next round downloads it again.
  if ! verify_staging; then
    echo "download incomplete or corrupt: retrying in 30 s" >&2
    sleep 30
    exec "$0"
  fi
  printf '%s\n' "$HF_REVISION" > "$STAGING/.revision"
  mv "$STAGING" "$MODEL_DIR"
  echo "model ready in $MODEL_DIR"
fi

# A model downloaded before the revision was pinned has no .revision file: it is the one
# that was on `main` at the time and is accepted as it is. One that records another revision
# is kept too (a download costs 4.5 GB); say so, since the voices may sound different.
if [ -f "$MODEL_DIR/.revision" ]; then
  have=$(cat "$MODEL_DIR/.revision")
  if [ "$have" != "$HF_REVISION" ]; then
    echo "WARNING: the model in $MODEL_DIR is revision $have, the pinned one is $HF_REVISION." >&2
    echo "WARNING: to download the pinned one, stop the service, delete the /models volume and start again." >&2
  fi
fi

exec nice -n "$NICE" /opt/qwen3-tts/qwen_tts -d "$MODEL_DIR" --serve 8080 -j "$THREADS" --max-request-seconds "$MAX_SECONDS"
