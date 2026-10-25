# Research workspace review

The review follows a mathematical workflow: read the definition, inspect assumptions, choose an operation, compare the result, then follow dependencies. It is an implementation review and native interaction check, not a usability study with recruited researchers.

| Observed issue | Implemented change |
| --- | --- |
| Source text, rendered input and rendered output competed vertically in every cell. | Saved cells lead with typeset mathematics. A pencil opens the original syntax and live preview; a successful apply/operation closes it; Discard restores the saved expression. Draft errors retain the editor and suppress outdated output. |
| Every cell showed the full operation set, variable input and JSON bindings. | Only the selected cell shows operations. Numeric evaluation opens explicit numeric fields for the free variables. Values are required and checked for finiteness before calling Rust. |
| Derivative controls displayed x even when another variable was supplied. | Controls follow the variable, using partial-derivative notation when other independent symbols occur. |
| Clicking an object did not reveal its notebook position. | Shared selection reveals and scrolls to the cell. A narrowly scoped backend navigation command reveals Notebook without relaxing general docking permissions. |
| The sidebar graph was clipped, and raw SEG IDs competed with mathematical content. | The selected cell's Uses / Used by links lead the inspector. The fitted graph and internal SEG remain expandable; the full graph window is retained. |
| Selection, recomputation and inspection repeatedly reset expanded reading sections. | Inspector disclosures and research-note expansion survive ordinary renders. |

Result trust, conditions and precision remain visible. Input and result are labelled separately: a derivative is not presented as an equality with its original function. Global assumptions, dimensions, branching, native 3D rendering and panel docking remain available. Existing design tokens and mathematical fonts are retained.

Native macOS verification covers mathematical editing, y differentiation, numeric substitution, narrow docked panes, floating/returning Notebook and selection navigation. Exact phone viewport certification and researcher-based accessibility/usability studies remain outside this review.
