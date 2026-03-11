# Visualizer Flow Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Rework the FFT visualizer motion so it reads as one smooth flowing surface instead of independent buckets with jumpy accents.

**Architecture:** Keep FFT analysis unchanged and confine the behavior change to `VisualizerFrameState` and the spectrum renderer in `src/ui.rs`. Drive rendering from a horizontally smoothed target surface, update per-bucket motion with damped attack/release behavior, then derive restrained peak and head accents that stay attached to the body.

**Tech Stack:** Rust, ratatui, cargo test

---

### Task 1: Add failing tests for the new motion model

**Files:**
- Modify: `tests/ui_render.rs`
- Test: `tests/ui_render.rs`

**Step 1: Write the failing test for horizontal coherence**

```rust
#[test]
fn spreads_an_isolated_spike_into_neighboring_buckets() {
    let mut frame_state = VisualizerFrameState::new(5);
    frame_state.advance(&[0.0, 0.0, 1.0, 0.0, 0.0]);

    assert!(frame_state.smoothed()[1] > 0.0);
    assert!(frame_state.smoothed()[3] > 0.0);
}
```

**Step 2: Write the failing test for attached top accents**

```rust
#[test]
fn keeps_the_top_accent_close_to_the_surface() {
    let mut frame_state = VisualizerFrameState::new(1);
    frame_state.advance(&[1.0]);
    frame_state.advance(&[1.0]);

    assert!(frame_state.orb_positions()[0] >= frame_state.smoothed()[0]);
    assert!(frame_state.orb_positions()[0] - frame_state.smoothed()[0] < 0.2);
}
```

**Step 3: Run tests to verify they fail**

Run: `cargo test --test ui_render spreads_an_isolated_spike_into_neighboring_buckets keeps_the_top_accent_close_to_the_surface`
Expected: FAIL because the current model does not spread isolated spikes and allows a much larger accent overshoot

### Task 2: Implement the new frame-advance model

**Files:**
- Modify: `src/ui.rs`
- Test: `tests/ui_render.rs`

**Step 1: Add target-smoothing helpers and any new motion constants**

```rust
fn smoothed_target(raw: &[f32], index: usize) -> f32 {
    // weighted average using the bucket and immediate neighbors
}
```

**Step 2: Update `VisualizerFrameState::advance`**

```rust
for index in 0..raw.len() {
    let target = smoothed_target(raw, index);
    // update surface value and velocity
    // update peak so it trails but stays attached
    // update top accent so it remains close to the surface
}
```

**Step 3: Run targeted tests**

Run: `cargo test --test ui_render spreads_an_isolated_spike_into_neighboring_buckets keeps_the_top_accent_close_to_the_surface`
Expected: PASS

**Step 4: Commit**

```bash
git add src/ui.rs tests/ui_render.rs
git commit -m "feat: smooth visualizer motion"
```

### Task 3: Update renderer expectations for restrained accents

**Files:**
- Modify: `src/ui.rs`
- Modify: `tests/ui_render.rs`
- Test: `tests/ui_render.rs`

**Step 1: Adjust rendering rules if needed**

```rust
let role = if distance_from_bottom < height_cells {
    VisualizerCellRole::Line
} else if top accent is within a restrained top band {
    VisualizerCellRole::Head
} else if peak is present {
    VisualizerCellRole::Peak
} else if glow is present {
    VisualizerCellRole::Glow
} else {
    VisualizerCellRole::Empty
};
```

**Step 2: Revise tests that describe old ballistic orb behavior**

```rust
#[test]
fn top_accents_do_not_detach_far_above_the_surface() {
    // assert restrained separation instead of large overshoot
}
```

**Step 3: Run the full visualizer test file**

Run: `cargo test --test ui_render`
Expected: PASS

### Task 4: Verify the relevant suite stays green

**Files:**
- Verify: `tests/ui_render.rs`
- Verify: `src/ui.rs`

**Step 1: Run focused tests for FFT and UI rendering**

Run: `cargo test --test ui_render --test analysis_fft`
Expected: PASS

**Step 2: Inspect the diff**

Run: `git diff -- src/ui.rs tests/ui_render.rs docs/plans/2026-03-11-visualizer-flow-design.md docs/plans/2026-03-11-visualizer-flow.md`
Expected: diff only covers the visualizer behavior and its planning docs
