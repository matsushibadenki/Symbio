# Handwritten formula MVP

## Workflow

Notebook → pen button → draw one formula → Recognize → correct the candidate in MathLive → Insert and calculate. Nothing is committed automatically. The source textarea mirrors MathLive for precise LaTeX repair; Rust-generated MathML previews only supported SEG input. An invalid preview disables insertion. A valid definition can still have an unsupported/domain calculation error after insertion, using the normal notebook behavior.

The small local model is fallible: a straight handwritten line was recognized as `|` in native QA and required correction. End-of-sequence status is not mathematical validation. Initial scope is a single formula, not paragraphs, multi-line derivations or a general matrix handwriting recognizer.

## Architecture and reuse

- `crates/handwriting`: independent Rust `Ink`, `Point`, `Recognizer` and `Recognition` API. No Tauri or mathematical-kernel dependency. It owns rasterization, ONNX encoder/decoder sessions and tensor buffers. CPU inference is the tested baseline; no CoreML/GPU acceleration is claimed.
- `apps/desktop/src/handwriting.rs`: small Tauri adapter. Rust keeps the current ink snapshot, generation and revision. Recognition rejects changed documents/revisions. Loading settings does not initialize models; Apply loads them. Paths persist in the application's config directory. Ink is transient, not stored in `.symbio` by this MVP.
- `apps/desktop/ui/handwriting.js`: host-independent editor shell with callbacks for model configuration, ink update, recognition, preview, commit and pending-draft notification. It imports locally vendored MathLive; no CDN is required. SVG is the low-volume input feedback, while image/tensor work stays Rust-side. Port this shell, its `.ink-*` stylesheet and MathLive assets together, or supply another frontend against the Rust crate.
- The host calls the same `math.define_latex` transaction used elsewhere. Confirmed LaTeX, SEG and calculated output use normal saving/undo. The recognition model is not a mathematical authority. Input and inference are separate from notebook edits.

Input has bounded strokes/points; normalized tensors do not cross IPC. UI requests carry ink revisions; stale preview replies are ignored. Pending ink/candidate drafts use the existing draft guard. Closing the dialog discards the transient ink/candidate and requests cancellation of active recognition. Model loading and inference run on blocking workers, not the UI thread. Original-ink persistence and confidence calibration remain future work. Recognition supports cooperative cancellation between native inference calls; model loading itself is not cancellable. Desktop recognition uses the reusable width-three beam search; greedy decoding remains available to SDK consumers.

## Development setup

No weights/runtime are redistributed with the repository. MathLive 0.111.4 and fonts are vendored for offline editing with license notices. Obtain the pinned optional model bundle:

```sh
python3 scripts/setup-handwriting.py /absolute/path/to/models
```

The script downloads ink-on v0.1.0 assets and validates pinned SHA-256; existing mismatched files are rejected. Download ONNX Runtime 1.23.2 separately from [Microsoft's official release](https://github.com/microsoft/onnxruntime/releases/tag/v1.23.2), matching the host architecture. In the dialog's local model settings, use the folder buttons to choose the model folder and runtime library with native dialogs, or edit their absolute paths directly. Cancel leaves the previous path unchanged. Selection alone does not execute the runtime; explicit Apply loads it. Document-generation checks reject a selection returned for a different document. On macOS ARM64 this is `libonnxruntime.dylib`. For this development workspace, ignored artifacts are under `.local/handwriting/models` and `.local/handwriting/runtime`.

The CoMER/ink-on model is an external development artifact. The ink-on application has an Apache-2.0 license, but the upstream checkpoint/training-data and derivative-weight redistribution chain has not been fully established in this work. Do not interpret the app code license as clearance to redistribute model weights. Release packaging and model-license clearance remain pending.

Model source: [ink-on v0.1.0](https://github.com/kimseungdae/ink-on/releases/tag/v0.1.0), based on [CoMER](https://github.com/Green-Wood/CoMER). No upstream inference source was copied into the Rust crate. Preprocessing and input/output conventions were studied from the published implementation.

| Artifact | SHA-256 |
| --- | --- |
| encoder_int8.onnx | 272f17c7b120db61db6cd8d77e695d0139144044abd57d9b23284435a5d916fa |
| decoder_int8.onnx | 658901d81ca3aa4b62b0da262e8fe67ebcfdac19cfb52ca2cfd076bebd481088 |
| vocab.json | 6ef75b588049981d2b5b2ceedcc7066dddcbb0c7d04ee2cbfb0ec861f192a850 |

The setup/SDK support a fixed model contract, not arbitrary ONNX models. Only load trusted runtime libraries; they are native executable code. Windows/Linux and pen-pressure hardware have not been tested.

## Model proof

```sh
python3 scripts/smoke-handwriting.py /absolute/models /absolute/runtime/library
```

The test runs the standalone native recognizer on three owned, synthetic stroke fixtures: `1`, `0`, `x+1`; checks the real model output and clean process exit; then sends recognized LaTeX through the real CLI into SEG and exact Symbolic results. These three fixtures are a transport/model smoke test, not an accuracy benchmark or a claim about real users' handwriting. They have no third-party dataset provenance. Native QA additionally exercised an ambiguous live stroke, MathLive correction to `1+1`, explicit insertion and exact result `2` in a separate test document.

## Editing safeguards

Drawing or undoing a stroke invalidates the current insertion candidate. Recognize again or explicitly correct the formula to obtain a new Rust preview before insertion. A delayed preview cannot re-enable a candidate invalidated by newer strokes. Insertion is disabled during model selection, loading, recognition and other pending actions, and while a stroke is active. Native selection is provided by the host through the `pickPath(kind)` callback; the editor shell and recognizer remain independent of Tauri.

## Alternative recognition candidates

The desktop displays up to three distinct model suggestions as typeset formula buttons. Select one to replace the MathLive draft and run a fresh Rust preview. Selection does not insert a notebook cell; insertion remains explicit. Changing/undoing strokes clears the candidate list and invalidates pending previews. Candidate display fields are inert so MathLive cannot intercept the surrounding selection button's click.

Search is bounded to width 3 and 128 steps (at most 384 decoder calls). Hypotheses are ranked by cumulative token log-probability, including the end token; no length normalization or confidence calibration is applied. This may prefer shorter hypotheses and does not establish mathematical correctness. Incomplete hypotheses can appear and still require review. A candidate need not be accepted by Symbio's supported LaTeX subset; the Rust preview gates insertion. The algorithm and image/tensor buffers remain in the standalone Rust crate, not JavaScript.

To repeat the real-model candidate/SEG smoke test:

```sh
python3 scripts/smoke-handwriting.py /absolute/models /absolute/runtime/library --beam
```

For the owned `x-plus-one` fixture, the tested model returned `x + 1`, `X + 1` and `X + l`. This illustrates correctable ambiguity, not an accuracy benchmark. Three synthetic fixtures retained the expected top candidate in both greedy and beam modes.

## Recognition cancellation

The stop icon is enabled while recognition is pending. Stop, or Escape during recognition, invalidates pending suggestions and requests cancellation; controls return when the worker acknowledges it. A native encoder/decoder call already running is allowed to finish, so cancellation is cooperative rather than immediate ONNX termination. Closing the editor also cancels its active recognition and clears the Rust ink snapshot. Cancellation does not insert or alter notebook cells. A late successful reply is ignored even if a host cannot cancel computation.

Reusable hosts can clone `CancellationToken`, pass it to `Recognizer::recognize_candidates_with_cancel`, and call `cancel()` from another thread. Cancellation returns the typed `Cancelled` error. Tokens are single-use; use a fresh token for a retry. Tauri registers its request token before scheduling the blocking worker, cancels superseded requests and scopes explicit cancellation to document generation/ink revision. Model configuration/loading and native file dialogs are outside this cancellation API.

The standalone example's `--cancel` mode verifies a pre-cancelled request is rejected and that the same loaded recognizer can run a fresh recognition. This complements synthetic search cancellation tests; it does not measure interruption latency inside native ONNX calls.
