# Screenshot feedback review

The supplied screenshots show an older interface. The useful product direction is a reproducible derivation/research notebook, with mature external CAS adapters as future computation backends. Broad Mathematica replacement claims are inappropriate for the current kernel.

| Feedback | Current response |
| --- | --- |
| Calculation versus substitution is unclear | Explicit Symbolic, exact Substitute and Numeric operations; partial rational substitution now computes and persists a replayable recipe. |
| JSON parameter entry | Already replaced by per-variable fields; exact and numeric inputs are separate. |
| Unnatural references and constant signatures | Definitions use `:=`; references render as linked mathematical symbols while retaining explicit source identity. |
| Dependent `z` appears as an input | Explicit model metadata separates inputs `x,y` from output `z`. |
| Missing free-variable assumptions | Input parameters are listed automatically with an explicit Unspecified domain until configured. |
| Internal expression IDs dominate the UI | Objects use typeset expressions; SEG details remain an optional inspector disclosure with terminology help. |
| Growing SEG | Existing structural interning and collection retain reachable expressions across branches and results. Document-wide counts are not the node count of a single formula. |
| Missing 3D axes/ranges | Axes, ticks and independent x/y ranges already exist. Publication export, color bars and adaptive sampling remain planned. |
| Correctness and provenance | Added bounded contract tests and an optional independent SymPy oracle. Formal certificates and general derivation traces remain planned. |
| Advanced physics and CAS breadth | Tracked as future work; no unsupported claims of tensor, noncommutative, general ODE/PDE or arbitrary precision support. |

See [Mathematical contract](mathematical-contract.md) for parsing, domains, substitution and evidence limits, and [Roadmap](roadmap.md) for priorities.
