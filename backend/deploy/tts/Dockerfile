# Speech engine for "read lessons aloud" (ADR 0013): Qwen3-TTS 1.7B CustomVoice served by the
# C engine gabriele-mastrapasqua/qwen3-tts (MIT), CPU only. Built from a pinned commit.
#
# The engine Makefile compiles with the SIMD level of the machine doing the build
# (-march=native on this path): build and run on the same host, as the compose file does.
# The model (~4.5 GB) is NOT in the image: entrypoint.sh downloads it into /models once.
ARG BASE=fedora:43

FROM ${BASE} AS build
# Bump this sha on purpose, after checking the engine flags in `qwen_tts -h` again.
ARG ENGINE_REPO=https://github.com/gabriele-mastrapasqua/qwen3-tts
ARG ENGINE_COMMIT=ef339be58a778b062e1c14382347964552eae007
RUN dnf -y install gcc make git openblas-devel && dnf clean all
WORKDIR /src
RUN git init -q . \
 && git fetch -q --depth 1 "$ENGINE_REPO" "$ENGINE_COMMIT" \
 && git checkout -q FETCH_HEAD \
 && make blas

FROM ${BASE}
# openblas-serial provides libopenblas.so.0, the library openblas-devel links against (as in the bench setup).
# curl and bash: used by the engine's own download_model.sh (resumable, one file at a time).
RUN dnf -y install openblas-serial curl bash && dnf clean all \
 && useradd --system --uid 10002 --home-dir /nonexistent --shell /sbin/nologin tts \
 && install -d -o tts -g tts /models
COPY --from=build /src/qwen_tts /src/download_model.sh /opt/qwen3-tts/
COPY entrypoint.sh /opt/qwen3-tts/entrypoint.sh
# The model is pinned too: download_model.sh fetches `resolve/main`, a moving branch, so the
# build rewrites that to the revision below (and fails if the script no longer has that URL,
# e.g. after bumping ENGINE_COMMIT). entrypoint.sh checks the download against the same revision
# and records it in /models/<dir>/.revision. A model already in the volume is never downloaded
# again: if its .revision differs from this one the entrypoint only warns (delete the volume to
# fetch the pinned one); one without .revision predates the pin and is accepted.
# Bump it on purpose, after listening to the voices again: the clip cache is keyed by model name,
# not by revision, so old clips would keep sounding like the old weights.
ARG MODEL_REVISION=0c0e3051f131929182e2c023b9537f8b1c68adfe
ENV TTS_MODEL_REVISION=${MODEL_REVISION}
RUN grep -q '/resolve/main"' /opt/qwen3-tts/download_model.sh \
 && sed -i "s#/resolve/main\"#/resolve/${MODEL_REVISION}\"#" /opt/qwen3-tts/download_model.sh \
 && ! grep -q 'resolve/main' /opt/qwen3-tts/download_model.sh \
 && chmod 755 /opt/qwen3-tts/entrypoint.sh /opt/qwen3-tts/download_model.sh
USER tts
VOLUME /models
EXPOSE 8080
ENTRYPOINT ["/opt/qwen3-tts/entrypoint.sh"]
