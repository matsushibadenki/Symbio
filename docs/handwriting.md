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

### Digit 1 and vertical bar

Collected four additional labeled inputs in an isolated macOS Tauri notebook: `11`, `|x|`, `|1|` and `1`. Combined them with the three existing labeled notebook inputs above. The saved logical stroke coordinates and labels are in [notebook-bars.jsonl](../examples/handwriting/notebook-bars.jsonl). The same seven recordings went through the original and updated Rust recognizers with the same local model and runtime. Matching uses the benchmark's LaTeX token normalization.

After beam search, the recognizer can move an existing complete candidate to first place when the leading complete candidate is a simple sum or difference with exactly one bare bar and the alternative replaces only that bar with `1`. The original candidates and model log scores are retained. Paired bars and explicit LaTeX commands are outside the rule.

| Reference | Correct rank before | Correct rank after |
| --- | ---: | ---: |
| `1+1` | 1 | 1 |
| `1-1` | 2 | 1 |
| `x+1` | 1 | 1 |
| `11` | Outside top three | Outside top three |
| `|x|` | 2 | 2 |
| `|1|` | 1 | 1 |
| `1` | 1 | 1 |

Top-1 rose from 4/7 to 5/7. Top-3 stayed at 6/7. The seven cases use fixed pointer paths and include the example that motivated the rule. This targeted comparison measures its effect on these saved inputs. Independent natural handwriting and broader vertical-bar notation still need evaluation.

### Independent check of digit-versus-bar ranking

Replay the MathWriting excerpt with `--paired` added to the benchmark command above. Each input is inferred once. The same candidate list is then evaluated in raw decoder order and notebook order. The JSON report includes changed, corrected and regressed first candidates, bar-bearing reference counts, and calibration per rank in both orders.

On the 100 validation and 100 test formulas, the rule activated zero times. Seven validation references and 15 test references contained a vertical-bar token. The test top-1 was 9/100 in each order and top-3 was 12/100 in each order. There were zero inference errors. This checks these independent inputs for changes. It provides no evidence about accuracy on independent examples where the rule actually activates.

The first-rank test Brier score was 0.08085 after calibration versus 0.08200 for the constant baseline. The second- and third-rank paired bootstrap intervals for Brier improvement included zero. Confidence percentages remain undisplayed. The next evaluation needs more independent short additions and subtractions whose raw candidates actually exhibit the digit/bar ambiguity.

### 2026-11-17 Targeted notebook follow-up

Collected 12 new labeled samples via pointer drags in a macOS Tauri test notebook. Eight were additions or subtractions using handwritten digits and variables, and four included absolute-value bars. The prompts were fixed before scoring. Captures with missing strokes were redrawn before saving. The stored [labeled strokes](../examples/handwriting/notebook-bars-followup.jsonl) were passed through a single ONNX inference per expression and evaluated in both orders.

```sh
python3 scripts/benchmark-handwriting.py examples/handwriting/notebook-bars-followup.jsonl /path/to/models /path/to/libonnxruntime.dylib /path/to/new-report.json --notebook
```

Top-1 went from 6/12 to 7/12. Top-3 was 10/12 in both orders. The rule activated once, promoting the correct `1-x` over `| - x`. Wrong promotions and correct-to-wrong regressions were both zero. The four absolute-value controls did not activate the rule. A wrong promotion counts any changed first candidate that is incorrect after adjustment, including changes between incorrect candidates.

This targeted set contains automated pointer paths and one activation. It gives a paired observation on the new inputs but leaves the general wrong-promotion rate undetermined.

### Human-written arithmetic and standalone bars

The targeted benchmark selects human-written MathWriting records by their reference labels, before running inference. It selects up to 150 records per category by sample-ID hash. Arithmetic references contain `1`, a plus or minus sign, and at most 16 normalized tokens from ASCII letters, digits and signs. Single-bar references include conditional-probability notation, standalone divisibility notation, and other single-bar uses. The conditional-probability category uses a probability-operator prefix and the divisibility categories use a bar between integers or an explicit top-level `\mid` between simple operands. These are notation-based filters.

```sh
python3 scripts/benchmark-handwriting.py /path/to/mathwriting-2024.tgz /path/to/models /path/to/libonnxruntime.dylib /path/to/bar-test.json --target-bars --exclude-excerpt /path/to/mathwriting-2024-excerpt.tgz --jobs 4
python3 scripts/benchmark-handwriting.py /path/to/mathwriting-2024.tgz /path/to/models /path/to/libonnxruntime.dylib /path/to/bar-train.json --target-bars --bar-split train --jobs 6
```

The official test subset excludes earlier excerpt IDs. The separate challenge subset draws from the dataset's human training partition to increase coverage, with no fitting of Symbio's rule on those samples. Each expression is inferred once and scored in both orders. A correct match requires the same normalized LaTeX tokens and a complete candidate. That scoring preserves differences between commands such as `\mid` and a literal bar. The study uses prompted human handwriting outside Tauri. Dataset files are loaded locally from the publisher's archive.

| Source and selected samples | First candidate before → after | Top three before → after | Activated | Correct promotions | Wrong promotions |
| --- | ---: | ---: | ---: | ---: | ---: |
| Official test, 142 | 5 → 5 | 13 → 13 | 0 | 0 | 0 |
| Separate challenge, 361 | 57 → 58 | 83 → 83 | 1 | 1 | 0 |

The test subset contained 4 arithmetic, 17 conditional-probability, and 121 other single-bar samples. The challenge subset contained 76 arithmetic, 135 conditional-probability, and 150 other single-bar samples. There were zero inference errors. The activation was on challenge sample `c500b74ac05b02fc` in the arithmetic category. None of the 423 single-bar samples triggered a change. The two selected-ID SHA-256 digests were `ef25e86e364b6d612d83f6814bdc89d9f7b6b2edbfbab9753e3722fb972f9ad1` (test) and `80dc05b73824981975d8d15f3775f6f936f0c24eced1eb788c4031771aaf0546` (challenge).

The supplementary divisibility pass matched 93 human-written records with a single bar in a sum or product index such as `p|n` or `d|n`. Both rankings scored 0/93 at top-1 and top-3 under full-expression exact match. There were zero activations and zero inference errors. This pass may overlap the challenge set and is reported separately. Reproduce it by adding `--indexed-divisibility` to the challenge command and choosing a fresh report path. The selected-ID SHA-256 is `723d054f6ffea5ad396b2549b064741d360a243510bf03d9a07056a3fde6bef4`. Zero wrong promotions with one activation leaves the wrong-promotion rate uncertain.

### Expanded activation audit

Use `--target-bars --expanded-arithmetic --bar-split valid` or `--bar-split train` with the full MathWriting archive. This includes all matching flat arithmetic references, even those without a digit 1. It keeps up to 150 per single-bar category in deterministic sample-ID order. Labels and split determine inclusion before inference. This selection can expose a wrong promotion when a recognizer hallucinates a 1 in an expression that never contained one. The validation run excludes the original excerpt IDs. The train partition is used as a separate challenge set and can overlap the previous challenge run.

The report estimates the correct- and wrong-promotion proportions among activations, with 95% Wilson intervals. A wrong promotion means the new first candidate is incorrect. This includes changes between two wrong candidates. A correct-to-wrong regression is additionally counted on its own. A split or cohort with no activations reports no promotion-rate estimate.

| MathWriting subset | Samples | Top-1 before → after | Top-3 before → after | Correct / wrong promotions |
| --- | ---: | ---: | ---: | ---: |
| Expanded validation | 178 | 11 → 12 | 22 → 22 | 1 / 0 |
| Expanded challenge | 519 | 126 → 127 | 172 → 172 | 1 / 0 |
| Of the two expanded sets, flat arithmetic | 250 | 119 → 121 | 159 → 159 | 2 / 0 |
| Of the two expanded sets, conditional probability | 147 | 8 → 8 | 21 → 21 | 0 / 0 |
| Separate indexed divisibility controls, previous paired run | 93 | 0 → 0 | 0 → 0 | 0 / 0 |

The expanded sets had zero inference errors. The rule activated twice in 697 distinct records, selecting the correct first candidate both times. The observed correct-promotion proportion was 2/2 (100%, Wilson 95% interval 34.2–100%), and the wrong-promotion proportion was 0/2 (0%, interval 0–65.8%). Activation IDs were `c7d062b6d66c2e22` (new validation example) and `c500b74ac05b02fc` (already observed in the previous challenge). Correctly transcribed references in the bar categories were unchanged. The independent divisibility controls can overlap the challenge and are excluded from the 697-record denominator. No ranking rule or model weights were changed.

Use the full archive and pinned model from the previous section. Run the challenge command with `--expanded-arithmetic` and a fresh output path. For validation use `--bar-split valid --expanded-arithmetic --exclude-excerpt /path/to/mathwriting-2024-excerpt.tgz`. The selected-ID SHA-256 digests are `54428e8326fc85a60e6d5af3e12d2e6939ba86cc0fd37fbb80ecafe542b025dc` (validation) and `99eef426987a4a2ec79591043a939b5904f701b82ee7ee573e233b6ed2278da6` (challenge).

### Conditional probability and divisor-index diagnostics

Reused the 147 conditional-probability inputs (12 validation and 135 challenge) and the 93 indexed-divisibility inputs from the previous paired runs. For each top-three miss, aligned normalized LaTeX tokens against the closest complete candidate by edit distance. If none was complete, used the closest available candidate. Each unmatched token edit is assigned once to a bar token, a subscript/superscript marker or argument, or a surrounding token, in that precedence order. These are transcription diagnostics rather than symbol segmentation labels.

| Baseline top-three misses | Bar edits | Script edits | Surrounding edits |
| --- | ---: | ---: | ---: |
| Conditional probability (126) | 78 | 100 | 265 |
| Indexed divisibility (93) | 93 | 154 | 164 |

The largest edit category in both cohorts was surrounding tokens. The most frequent individual edit in the divisibility cohort was `\prod` read as `\pi` (48 edits). The checkpoint vocabulary has no `\prod` token and 77 references require it. In conditional probability, surrounding errors span multiple symbols, including 15 omitted vector commands. The most common individual bar substitution was `|` read as `/` (25 edits).

The new correction recovers `\prod` only from a complete pi candidate with a grouped divisor index using `d` or `p` and `n`, and a following factor that uses the same divisor. Within that pattern it can also repair a separator confused with the vertical bar. The rule uses context and retains the original model scores. It does not add tokens to the ONNX vocabulary or change the model. The fixed patterns were developed on these examples and need independent evaluation.

| Same recorded inputs | Top-1 before → after | Top-3 before → after |
| --- | ---: | ---: |
| Conditional probability, 147 | 8 → 8 | 21 → 21 |
| Indexed divisibility, 93 | 0 → 5 | 0 → 11 |

There were zero inference errors. Use the prior MathWriting commands with `--bar-cohort conditional_probability` on the validation and train partitions, and use `--bar-split train --indexed-divisibility` for divisibility. Add `--diagnostic-rows /path/to/new-rows.jsonl` to each run. Process each local file with `python3 scripts/diagnose-handwriting.py /path/to/new-rows.jsonl --order before` or `--order after`. The baseline order in this tool is raw decoder order. Prior digit/bar reranking did not change the 240 references in this comparison.

### Independent products and conditional slash alternatives

The independent product check used all 19 human validation records with a single bar and a product index of the form `\prod_{a|b}` for single-letter operands. No sample ID overlaps the 93 development records. Formula forms can overlap. Two candidate lists were changed by the previous product correction. In one, the correction restored the full formula as candidate 2. In the other, it restored the product and the divider but the surrounding `\gamma` was still read as `V`.

The conditional-probability failures showed both `P(A/B)` and `P(A|B)` hypotheses for some inputs. In other inputs the correct formula differed from the first candidate only at that separator and the raw beam lacked the correct variant. This identifies a decoding ambiguity at the separator. It does not isolate the visual features that caused the model to emit a slash.

For complete candidates, the recognizer now offers a bar variant for a well-bracketed `P(...)` or `Pr(...)` call with exactly one outer slash, symbolic tokens on both sides, and no existing outer bar. It preserves the original first candidate in the three suggestions. It leaves numeric-only sides, nested divisions and multiple ambiguous probability calls alone. A symbolic ratio is still possible in this context and can be selected from the original leading candidate. The variants inherit the source candidate's model score.

| Paired evaluation | First candidate before → after | Top three before → after |
| --- | ---: | ---: |
| Independent indexed products, 19 | 0 → 0 | 0 → 1 |
| Conditional-probability development set, 147 | 8 → 16 | 21 → 29 |
| Independent official-test conditional probabilities, 18 | 0 → 0 | 0 → 0 |

The product baseline is raw model output and its comparison applies the existing product rule. The probability baseline includes the existing product and digit rules, and its comparison adds the slash rule. Neither probability cohort had a previously correct top-1 or top-3 match regress. In the nearest-candidate edit diagnostic, bar-to-slash substitutions fell from 25 to 8 in the development cohort and from 4 to 2 in the independent cohort. This did not recover any full formula in the independent probability cohort. Additional real symbolic-ratio controls are still needed.

Use `--target-bars --bar-split valid --indexed-products --diagnostic-rows PATH` to reproduce the product selection and `--target-bars --bar-cohort conditional_probability --diagnostic-rows PATH` for the independent probability test. The latter includes all 18 qualifying test records, including the one in the public excerpt.

### Independent conditional-density and genuine slash audit

Selected 193 human-written MathWriting records by reference LaTeX. The 149 lowercase `p(...|...)` references extend evaluation beyond the earlier uppercase probability samples. None of the 193 IDs overlapped the 184 records in the preceding product and probability comparison. The controls contain 38 whole-expression alphabetic ratios such as `A/B`, and 6 genuine slashes inside `P(...)` or `Pr(...)`, including forms such as `Pr(X_n/n>0)`. The samples come from MathWriting's train, validation, and test partitions and are pooled by notation. This audit used no fitting on the selected inputs.

The benchmark applies the existing digit and product rules in both branches. It enables only the conditional-bar rule in the second branch. Both branches use the same ONNX beam. The correct-slash count requires a complete, exact first-candidate match before adjustment and a first candidate that differs only by replacing one correct slash with a vertical bar afterward.

| Cohort | Top-1 before → after | Top-3 before → after | Correct slash changed to bar |
| --- | ---: | ---: | ---: |
| Conditional density, 149 | 4 → 4 | 5 → 5 | 0 |
| Whole-expression symbol ratio, 38 | 15 → 15 | 21 → 21 | 0 |
| Slash inside a probability-shaped call, 6 | 1 → 1 | 1 → 1 | 0 |

There were no inference errors. The rule changed nine already-incorrect first candidates in the density cohort and two already-incorrect first candidates in the probability-shaped controls. None became correct. Only one of the six in-call controls was correct at baseline, so zero correct-slash conversions provides limited evidence for that context. No production correction behavior was changed.

```sh
python3 scripts/benchmark-handwriting.py /path/to/mathwriting-2024.tgz /path/to/models /path/to/libonnxruntime.dylib /path/to/conditional-audit.json --conditional-audit --jobs 6
```

### Preserve recognized slashes in probability-shaped calls

Added two independently recorded macOS Tauri pointer controls in [notebook-slash-controls.jsonl](../examples/handwriting/notebook-slash-controls.jsonl). These are deliberately constructed slash inputs with the labels `P(A/B)` and `P(X/Y)`. Compared both versions on the frozen points and the pinned ONNX model. The old rule put `P(A|B)` first and retained `P(A/B)` second. The revised rule keeps `P(A/B)` first. It also keeps the model's existing second candidate `P(X/Y)`, which the old rule converted to `P(X|Y)`.

The recognizer now adds a contextual vertical-bar variant only to an unfilled slot. Existing suggestions keep their order and text under this rule. This protects model candidates with real slashes and reduces the number of conditional variants when the beam is already full.

| New Tauri pointer controls | Old → revised |
| --- | ---: |
| First candidate exactly correct | 0/2 → 1/2 |
| Exact match among the first three candidates | 1/2 → 2/2 |
| Correct slash changed to bar at rank 1 | 1 → 0 |

The rank-1 conversion count compares each version to the same decoder with the conditional rule disabled. For a conversion, the disabled-rule first candidate must match the reference, and the enabled-rule candidate must differ by exactly one slash-to-bar substitution.

Save the prior Rust benchmark executable before rebuilding the revised version. Reproduce the notebook comparison with `--notebook --prior-conditional-binary /path/to/prior-benchmark` and the new JSONL file. Use `--conditional-audit --prior-conditional-binary /path/to/prior-benchmark` for the MathWriting audit. Supply a fresh report path for each run.

The 193-sample MathWriting replay showed unchanged exact-match counts for conditional densities (4/149 first, 5/149 top three), slash contexts inside probability-shaped calls (1/6 for both), and standalone ratios (15/38 first, 21/38 top three). The 147-sample conditional-probability development set fell from 16 to 8 first-candidate matches and from 29 to 21 top-three matches. The independent 18-sample conditional set remained at zero matches for both metrics.

### Use stroke geometry to recover conditional bars

For complete probability-call candidates with a single eligible slash, the notebook now checks the original strokes before changing the separator. It looks for one straight stroke with nearby ink on both sides and a clear gap around the stroke. More than one qualifying stroke makes the geometry ambiguous. The rule requires the endpoint horizontal displacement to be at most 14% of the vertical displacement (about eight degrees from vertical). The spare-slot rule still applies when the geometry is inconclusive. The shape check operates in ink coordinates and does not change the ONNX model, vocabulary, or scores.

MathWriting tests use human-drawn ink. The public cohort selection uses reference formulas and does not supply separator stroke annotations. A vertical stroke in a multi-symbol formula may belong to another symbol, so this check measures the effect on recognition rather than the isolated accuracy of a separator classifier.

To replay the 18 official-test conditional formulas, use the same archive and pinned models with `--target-bars --bar-cohort conditional_probability --prior-conditional-binary /path/to/prior-benchmark`. Use `--conditional-audit` in place of the cohort options for the public slash controls.

| Same ink before and after | First candidate | First three candidates |
| --- | ---: | ---: |
| Conditional-probability development set, 147 | 8 → 13 | 21 → 25 |
| Independent official-test conditional probabilities, 18 | 0 → 0 | 0 → 0 |
| Independent indexed products, 19 | 0 → 0 | 1 → 1 |
| Tauri pointer-drawn slash controls, 2 | 1 → 1 | 2 → 2 |

Among top-three misses, the closest-candidate diagnostic counted 25 → 18 bar-to-slash edits on the development set and 4 → 3 on the independent conditional set. Top-three misses can also contain surrounding-symbol errors. The Tauri slash controls had 0 → 0 conversions of an otherwise correct first candidate from slash to bar. All these replays used the same checkpoint and had zero inference errors.

The separate 193-record MathWriting audit also had zero inference errors. The six human-written probability-shaped slash controls stayed at 1/6 for both exact-match metrics and 0 → 0 incorrect first-candidate slash-to-bar conversions. The 38 standalone ratios stayed at 15/38 first and 21/38 top three, with no incorrect first-candidate conversions. The 149 conditional densities stayed at 4/149 first and 5/149 top three. Three density first candidates changed without recovering the full expression.
