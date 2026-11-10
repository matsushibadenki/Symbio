# Native handwriting recognition

A GUI-independent Rust crate for bounded stroke rasterization and CoMER ONNX inference. It does not depend on Tauri, Symbio, SEG or MathLive, read research documents, download weights or modify files. The `image` dependency is used only for small, bounded CPU normalization. Tensors and model buffers remain native.

```rust,ignore
use symbio_handwriting::{Ink, Recognizer};
let mut engine = Recognizer::load(models_directory, onnx_runtime_library)?;
let candidate = engine.recognize(&ink)?;
// Treat candidate.latex as a suggestion; let the user edit/confirm it.
```

`Ink` uses logical x/y coordinates, with strokes grouped by pen-down/up. Limits: 256 strokes, 16384 finite points, coordinates bounded to ±10000. Rasterization produces white ink on black, height 256, width at most 1024. The content-height target is 128. Encoder mask values identify padded rows. The existing `recognize` / `recognize_image` methods retain greedy decoding. `recognize_candidates(&Ink)` uses beam width 3, at most 128 steps and 384 decoder calls, with a repetition stop. It returns at most three distinct LaTeX candidates in cumulative token log-probability order (including EOS), without length normalization. `Recognition.latex`, `complete` and `tokens` mirror the first candidate; `candidates` includes each suggestion and its uncalibrated `log_score`. Scores are not correctness probabilities. `complete` only means an end token was seen. Confidence calibration remains unfinished. Recognition cancellation is cooperative, between ONNX calls; an in-flight native call is allowed to finish.

The supported model contract is the ink-on v0.1.0 CoMER encoder/decoder and its 113-token vocabulary. The Rust implementation is independent; upstream architecture, preprocessing and vocabulary are described at https://github.com/kimseungdae/ink-on. Model files are not included. See the host project's `docs/handwriting.md` for pinned provenance and deployment limitations.

This pins ort 2.0.0-rc.11 and uses an explicitly supplied ONNX Runtime 1.23.2 library. rc.10 was rejected because of a macOS exit crash fixed upstream in rc.11. Telemetry is disabled. Hosts must configure a consistent runtime per process; loading arbitrary model formats or changing runtime libraries in an active process is not supported.

The `recognize` example is a standalone consumer, reusable without the desktop:

```sh
cargo run --locked --offline -p symbio-handwriting --example recognize -- MODELS RUNTIME INK.json
```

Pass `--beam` after `INK.json` in the standalone example to exercise the same candidate search used by the desktop. The default example retains greedy decoding.

For cancellation, create a `CancellationToken::default()`, clone it into the host's cancellation handler and call `recognize_candidates_with_cancel(&ink, &token)`. `token.cancel()` is thread-safe and cancellation returns a typed `Cancelled` error; use a new token for each subsequent request. This does not require Tauri or an async executor. The example's `--cancel` mode tests pre-cancellation followed by a fresh retry in the same recognizer. Loading models is not cancellable.
