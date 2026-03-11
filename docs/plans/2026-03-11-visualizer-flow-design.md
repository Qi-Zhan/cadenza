# Visualizer Flow Design

**Date:** 2026-03-11

**Goal:** Make the FFT visualizer feel more natural and continuous by improving both vertical motion inertia and horizontal coherence between adjacent frequency buckets.

## Problem

The current visualizer has three visible artifacts:

- Each bucket reacts too independently, so narrow spikes read as jitter instead of a continuous surface.
- Vertical motion is driven by fixed-frame interpolation, which makes rises and falls look mechanical.
- The `orb` accent can detach from the body and read as a separate bouncing particle instead of a subtle top highlight.

## Constraints

- Preserve the current TUI layout and the existing `needle + head + peak + glow` visual language.
- Do not change FFT analysis or bucket generation in `src/analysis.rs`.
- Keep the baseline continuous and avoid visual holes in rendered columns.
- Limit work to visualizer state advancement and rendering rules in `src/ui.rs`, with tests in `tests/ui_render.rs`.

## Recommended Approach

Use a wave-surface model instead of directly rendering per-bucket values:

1. Treat raw FFT buckets as input only.
2. Build a horizontally smoothed target surface so adjacent buckets influence each other.
3. Move the rendered surface toward that target with damped motion and asymmetric attack/release behavior.
4. Keep `peak` as a restrained trailing accent.
5. Convert the current `orb` into a soft top highlight that stays attached to the surface instead of launching above it.

This keeps the current style but shifts the motion from "independent columns with effects" to "one connected surface with accents."

## Frame Update Model

Each `advance(raw)` call should run in this order:

1. Compute a horizontally smoothed target using each bucket and its immediate neighbors.
2. Update the main surface with per-bucket value and velocity, using faster upward response than downward release.
3. Update `peak` as a slowly falling highlight that never drops below the surface.
4. Update the top highlight so it sits just above the surface, with only a small offset.

## Rendering Rules

- Fill each active column contiguously from the baseline to the current surface.
- Allow at most a restrained stack near the top: `head`, `peak`, then `glow`.
- Do not allow top accents to drift far away from the body.
- Preserve width, height, and bottom-row continuity across the panel.

## Testing Strategy

Add or revise tests to cover:

- Horizontal smoothing of isolated spikes into neighboring buckets.
- Strong hits producing smooth, attached top accents rather than detached jumps.
- Continuous decay without collapse or gaps.
- Existing rendering guarantees: baseline continuity, contiguous columns, full width, and full height.

## Risks

- Too much horizontal smoothing could blur meaningful spectral structure.
- Too much damping could make the visualizer feel sluggish instead of smooth.
- Existing `orb`-specific tests will need to be rewritten to match the new, less ballistic behavior.
