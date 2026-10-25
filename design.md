<!-- Hallmark · pre-emit critique: Philosophy 5 · Hierarchy 5 · Execution 4 · Specificity 5 · Restraint 5 · Variety 4 -->
# Symbio interface design

The existing light research-workspace system is retained. This document governs the desktop WebViews; mathematical documents, IPC ownership, dock layout and native GPU rendering remain in Rust.

## Reading order

Objects and assumptions at left establish context. The broad central notebook shows authored mathematics, an explicit operation and its result. The right inspector explains the selected cell's references and verification status. Notebook and 3D remain central tabs; panels remain dockable.

A saved equation is readable mathematics first. Source syntax, keyboard tools and numeric parameter fields appear when requested; JSON remains an API representation, not a required researcher input. Select one cell to expose its calculation controls. Do not imply equality between an input and a derivative or an approximate result: label definitions and results separately.

## Tokens and typography

Use `apps/desktop/ui/tokens.css`: existing white/neutral surfaces, restrained teal accent, system UI font, STIX/Cambria mathematical stack and code font for editable syntax. Math is the largest content. Headings stay upright. Trust, conditions and precision stay visible beside results; do not promote CAS output to formal proof.

## Interaction

Selection is shared Rust state. Selecting an object or reference reveals the corresponding notebook cell. Source drafts stay local until applied; errors keep their source editor open and suppress outdated outputs. Keyboard entry and source Undo remain available. Document Undo/Redo remains Rust-owned.

The inspector defaults to readable reference relationships. Full DAG exploration opens its existing window. SEG diagnostics and audit history remain expandable, retaining their open state across ordinary renders. Explanations belong in information tooltips.

## Layout and accessibility

Use compact rules and spacing instead of heavy cards. Mathematical expressions may scroll horizontally within their own region; UI labels wrap by meaning and controls wrap at narrow panel widths. Preserve focus rings, keyboard activation, accessible names, native MathML and reduced-motion preferences. Desktop native checks target the existing 1280×800 workspace; narrow browser panel checks cover 320/375/414/768 px where available.
