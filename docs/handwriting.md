# Handwritten formula MVP

## Workflow

Notebook → pen button → draw one formula → Recognize → correct the candidate in MathLive → Insert and calculate. Nothing is committed automatically. The source textarea mirrors MathLive for precise LaTeX repair; Rust-generated MathML previews only supported SEG input. An invalid preview disables insertion. A valid definition can still have an unsupported/domain calculation error after insertion, using the normal notebook behavior.

The small local model is fallible: a straight handwritten line was recognized as `|` in native QA and required correction. End-of-sequence status is not mathematical validation. Initial scope is a single formula, not paragraphs, multi-line derivations or a general matrix handwriting recognizer.

## Architecture and reuse

- `crates/handwriting`: independent Rust `Ink`, `Point`, `Recognizer` and `Recognition` API. No Tauri or mathematical-kernel dependency. It owns rasterization, ONNX encoder/decoder sessions and tensor buffers. CPU inference is the tested baseline; no CoreML/GPU acceleration is claimed.
- `apps/desktop/src/handwriting.rs`: small Tauri adapter. Rust keeps the current ink snapshot, generation and revision. Recognition rejects changed documents/revisions. Loading settings does not initialize models; Apply loads them. Paths persist in the application's config directory. Confirmed strokes are stored with the cell in `.symbio`. The notebook loads those points on demand when reopening the editor.
- `apps/desktop/ui/handwriting.js`: host-independent editor shell with callbacks for model configuration, ink update, recognition, preview, commit and pending-draft notification. It imports locally vendored MathLive; no CDN is required. SVG is the low-volume input feedback, while image/tensor work stays Rust-side. Port this shell, its `.ink-*` stylesheet and MathLive assets together, or supply another frontend against the Rust crate.
- The host calls the same `math.define_latex` transaction used elsewhere. Confirmed LaTeX, SEG and calculated output use normal saving/undo. The recognition model is not a mathematical authority. Input and inference are separate from notebook edits.

Input has bounded strokes/points; normalized tensors do not cross IPC. UI requests carry ink revisions; stale preview replies are ignored. Pending ink/candidate drafts use the existing draft guard. Closing the dialog discards unapplied changes and requests cancellation of active recognition. Model loading and inference run on blocking workers, not the UI thread. Confidence calibration remains future work. Recognition supports cooperative cancellation between native inference calls; model loading itself is not cancellable. Desktop recognition uses the reusable width-three beam search; greedy decoding remains available to SDK consumers.

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

Use the stroke icon on a notebook cell to reopen its saved handwriting and current formula. **Update and calculate** applies both to the same cell. Clear the strokes and apply a typed formula to remove the attachment. Undo restores the previous formula and strokes. Older documents open without handwriting attachments. The native document stores the strokes, while MathJSON, OpenMath and reproduction packages carry the mathematical content. Documents allow up to 16,384 points per cell and 64,000 points across all branches.

## Evaluating real handwriting

The benchmark uses human InkML samples from Google's [MathWriting 2024 excerpt](https://github.com/google-research/google-research/tree/master/mathwriting). Download the excerpt separately under its CC BY-NC-SA 4.0 data license. The benchmark reads the archive locally and does not extract or ship the samples.

```sh
python3 scripts/benchmark-handwriting.py /path/to/mathwriting-2024-excerpt.tgz /path/to/models /path/to/libonnxruntime.dylib /path/to/new-report.json
```

Each sample is scaled uniformly to the logical handwriting pad and processed by the same Rust beam recognizer used in the desktop app. A hit requires a complete candidate and a token match against the dataset's normalized LaTeX label after limited presentation normalization. The report treats inference errors as misses. This metric counts transcription matches, including case and symbol order. It does not compare symbolic equivalence.

The report fits separate monotone logistic mappings for the events “first candidate matches” and “at least one of the first three candidates matches.” The inputs are geometric mean token probabilities from completed candidates, including EOS. The first mapping uses the first candidate's score, and the second uses the maximum among the three. The official validation split determines the weights. The test split measures accuracy, Brier score and five-bin expected calibration error. The report identifies the exact archive, model files and runtime by SHA-256.

### MathWriting excerpt results

The published excerpt contains 100 human validation and 100 human test samples. Archive SHA-256: `cba038def001480a89962b25cb20a60df4c4145e94c86ef9d2af65f192cb82bc`. The pinned CoMER INT8 model and macOS ARM64 ONNX Runtime 1.23.2 processed all 200 samples.

| Match event | Validation hits | Test hits | Test Brier, raw → calibrated | Test ECE, raw → calibrated | Mean calibrated test probability |
| --- | ---: | ---: | ---: | ---: | ---: |
| First candidate | 10/100 | 9/100 | 0.7395 → 0.0808 | 0.8143 → 0.0102 | 10.02% |
| Any of the first three | 15/100 | 12/100 | 0.7131 → 0.1042 | 0.7844 → 0.0301 | 15.01% |

For the logistic mapping `sigmoid(bias + weight * score)`, the fitted pairs are `(-3.3517831441, 1.2763056950)` and `(-3.1031363271, 1.5096059224)`, respectively. Calibration changes the reported probabilities while preserving the original ranking.

All calibrated test predictions fell in the lowest (0–20%) reliability bin. Predicting the validation hit rate for every test sample gives Brier scores of 0.0820 and 0.1065. The fitted mappings provide only a small improvement over that reference. This run is a preliminary evaluation on the excerpt. The desktop keeps the model ranking without confidence percentages pending evaluation on a larger and more representative set of user inputs.

### Short-expression follow-up

The follow-up samples human pen traces from the official MathWriting 2024 validation and test directories, excludes IDs used in the earlier excerpt, and keeps formulas with at most 16 normalized LaTeX tokens. The token limit approximates short notebook entry. The traces come from prompted copying on touchscreens or digital pens, so results apply to that proxy population. The benchmark chooses samples by SHA-256 order of their IDs before inference and records split digests.

```sh
python3 scripts/benchmark-handwriting.py /path/to/mathwriting-2024.tgz /path/to/models /path/to/libonnxruntime.dylib /path/to/short-report.json \
  --max-tokens 16 --limit-per-split 300 --exclude-excerpt /path/to/mathwriting-2024-excerpt.tgz --jobs 4
```

The report also fits one calibrator for each displayed rank. A rank's denominator includes only samples where that rank exists. The constant baseline predicts that rank's validation match rate for every test candidate. The paired bootstrap interval estimates the difference in test Brier loss between the calibrator and that baseline, using 2,000 sample-level resamples. Negative values favor calibration. Per-rank outputs describe marginal match rates and can differ from a coherent distribution over a candidate list.

The follow-up processed 300 validation and 300 test formulas with zero inference errors. The full archive SHA-256 is `f1977f120b0efa8abf715ac544b0596b213b099133da99b5e64f946730929269`.

| Match event | Validation | Held-out test |
| --- | ---: | ---: |
| First candidate | 46/300 (15.3%) | 54/300 (18.0%) |
| Any of the first three | 59/300 (19.7%) | 69/300 (23.0%) |

| Rank | Test matches | Calibrated probability range | Constant → calibrated Brier | Paired Δ 95% interval |
| --- | ---: | ---: | ---: | ---: |
| 1 | 54/300 | 10.4–17.1% | 0.14831 → 0.14419 | −0.00519 to −0.00312 |
| 2 | 12/300 | 3.0% throughout | 0.03850 → 0.03850 | Includes zero |
| 3 | 3/300 | 1.33% throughout | 0.00991 → 0.00991 | Includes zero |

All three candidates were present for every test sample. The first rank improved slightly over the constant baseline, but every calibrated prediction fell below 20%, and the first rank's mean prediction was 15.4% against an observed 18.0%. The second and third rank mappings reduce to the validation match rate. This supports keeping the candidate list without percentages while testing better score features and collecting inputs from the actual notebook workflow. The earlier broad excerpt and this short-expression selection have different difficulty distributions, so their accuracy difference cannot be attributed to a recognizer improvement.

### Notebook stroke capture and decoder comparison

Two saved macOS Tauri test cells were replayed through the local CoMER model. The clean four-stroke `1+1` returned `1+1` first. The other recording contained a diagonal segment between the left vertical stroke and the horizontal stroke of the plus sign. The model returned `|+1`, `|+|` and `u+1`. Splitting the joined stroke at the boundary while keeping every observed point restored `1+1` as the first candidate. The finalized LaTeX in that notebook was manually edited for a prior workflow test, so it was not used as a recognition label. These recordings diagnose an input issue and do not estimate notebook-wide accuracy.

The pad now tracks the active pointer ID, ends the stroke on capture loss or release, and starts a fresh stroke on each primary press. A regression test reproduces a missing release and verifies the two resulting strokes stay separate.

The decoder experiments used the same 300 held-out MathWriting sample IDs, weights and runtime as the short-expression follow-up. The length-penalty experiment divided cumulative log probability by the square root of the scored token count, including EOS. The wider beam experiment kept six internal hypotheses and returned three candidates by cumulative log probability. Neither experiment changed the aggregate test accuracy.

| Decoder | First candidate | Any of the first three |
| --- | ---: | ---: |
| Baseline width 3 | 54/300 (18.0%) | 69/300 (23.0%) |
| Square-root length penalty | 54/300 (18.0%) | 69/300 (23.0%) |
| Internal width 6 | 54/300 (18.0%) | 69/300 (23.0%) |

Symbio retains the original decoder and ships the pad fix. Replaying pre-recorded InkML cannot exercise pointer capture, so the aggregate accuracy table evaluates decoder changes only. The point-preserving split of the joined notebook stroke isolates the capture issue on that one sample. The event handling has automated regression coverage and was verified in the native macOS comparison below.

### Native notebook stroke comparison

On macOS, used the same pointer drag paths and order in isolated Tauri builds before and after the capture change, with the same local model and runtime. Both saved documents contained the expected 4, 3 and 5 separate strokes. Each stored path matched its intended straight segment without a cross-stroke connector, and corresponding endpoint distances matched to one decimal place.

| Reference | Strokes before / after | Correct candidate rank before / after |
| --- | ---: | ---: |
| `1+1` | 4 / 4 | 1 / 1 |
| `1-1` | 3 / 3 | 2 / 2 |
| `x+1` | 5 / 5 | 1 / 1 |

The first candidate matched in 2/3 formulas and the top three contained the reference in 3/3 formulas in each build. The first candidate for `1-1` was `| - 1` in both. The historical connector did not recur in the paired baseline draws. The missing-release regression test exercises that condition explicitly. An intermediate native run caught a separate issue where the automation delivered a move with a zero button bitmask and the editor turned the gesture into a dot. The final build uses capture and pointer identity for stroke boundaries and accepts those moves. These three fixed pointer paths are a native workflow check with limited coverage of natural handwriting.
