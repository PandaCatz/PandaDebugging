---
name: game-visual-work
description: Required procedure for any task that changes what a player sees in a game - rendering, lighting, shaders, post-processing, particles, weather, atmosphere, scene dressing, prop placement, sprite or art integration, camera work, screen effects, or UI drawn over gameplay. Use when asked to make a game look better, add visual polish, fix something that "looks wrong" or "looks off", improve an environment, add effects, or integrate new art. Enforces establishing frame capture before editing, rendering the full level matrix before claiming success, and locking tuned values behind tests.
---

# Visual work on a game: required procedure

## Scope

Applies to any task that changes what the player sees: rendering, lighting, effects,
post-processing, scene dressing, prop placement, art integration, camera, or UI drawn
over gameplay.

Visual work fails in a specific way: it looks fine in the one frame you happened to
look at, and wrong everywhere else. This procedure exists to catch that.

## Phase 0 — Observability (blocking)

**Do not modify rendering code until you can produce an image of the current output.**

1. Try the environment's screenshot tool.
2. If it fails, build a capture path. Extract the framebuffer (`canvas.toBlob`,
   render-to-file, engine screenshot API), write it to disk, read it back as an image.
3. Verify capture works on unmodified code before making any edit.

If frames only advance under `requestAnimationFrame` and the surface is not compositing
(hidden pane, headless, backgrounded tab), rAF will never fire. Find a synchronous render
path instead — a resize handler, an explicit `render()`, a step function — and drive that.

Report the capture method you established.

**Locate or add a state-injection API.** One call each to set level, position, health,
weather, animation state, viewport. If reaching a state requires playing the game, you
will undertest that state. Add the hooks rather than working around their absence.

## Phase 1 — Baseline

Capture output **before any edit**. Describe what already works and name the parts you
must not degrade. You cannot tell whether you improved something without this.

Do not assume existing code is naive. Where it looks wrong, determine what constraint it
encodes and state that constraint before changing it. Coupled values, fixed scroll rates,
shared transforms, and single-speed layers are usually load-bearing — they encode a
relationship in the source art that breaks visibly when separated.

## Phase 2 — Draw order

Write the draw order out explicitly before implementing anything. Use this order unless
you have a stated reason to deviate:

1. **Sky / clear** — base gradient or clear colour. Nothing else may issue a
   full-surface fill after this point; that is what silently erases the layers below.
2. **Far parallax** — clouds, distant skyline, anything at the slowest scroll rate.
3. **Main background art** — the painted or tiled backdrop that defines the scene.
4. **Background-integrated animation** — water shimmer, distant traffic, sign flicker.
   Belongs *with* the backdrop so it inherits the same depth, not on top of everything.
5. **Atmospheric perspective** — ground fog, haze band sitting on the horizon line.
6. **Ground plane** — floor, its shading and decals.
7. **Back atmosphere** — the half of the weather that falls behind the actors.
8. **Depth-sorted world layer** — props, scenery, pickups, enemies, the player, ground
   dust, world-space markers and exits. See the invariant below.
9. **Combat and impact effects** — sparks, debris, shockwaves, floating numbers.
10. **Front atmosphere** — the half of the weather that passes in front of the actors.
11. **Foreground parallax** — closest scenery, fastest scroll, partially cropped by frame
    edges.
12. **Lighting / grade** — ambient tint, key wash, ground bounce, vignette. This unifies
    everything above it into one image, which is why it goes here and not earlier.
13. **HUD and UI** — deliberately outside the grade so it stays legible.
14. **Post-FX** — impact flash, grain, full-screen colour effects. Decide explicitly
    whether these cover the HUD; both answers are valid, an accident is not.

**Invariant — one depth-sorted list.** Everything that shares the play plane goes into a
single list sorted by depth, drawn in one pass. If scenery draws in a separate pass from
characters, no amount of tuning will ever sort them correctly against each other.

**Invariant — parallax increases monotonically.** Scroll rate must rise with layer index
from step 2 to step 11. Assert this if the rates are data-driven.

**Invariant — layers cut from one source art move together.** If a backdrop was sliced
into near/mid/far masks from a single painting, they must share one camera transform.
Scrolling them at different rates tears the geometry apart. Depth comes from adding new
layers around such a backdrop, never from splitting it.

## Phase 2b — Tuning

- Start every effect intensity at roughly half your instinct. You are staring at the
  effect in isolation; the player is looking at the whole game.
- Preallocate anything that spawns per frame. Fixed-size pool, active flag, recycle.
  Never allocate in the render loop.
- Cache per-frame constructed objects — gradients, paths, layout math. Key the cache on
  what actually invalidates it and rebuild only then.
- Split screen-space effects around the actor plane: some behind the characters, some in
  front. Effects that only draw in front read as being on the camera lens.
- Accessibility and reduced-motion settings must **reduce density, never disable the
  layer**. A disabled layer makes the game look broken for the people who need the setting.

## Phase 3 — Verification (blocking)

**Render the full matrix into a single contact sheet.** Every level, times every relevant
variant (character, weather, time of day), plus minimum and maximum viewport. Inspect the
grid, not individual frames — visual failures are usually relative, and one frame gives
you nothing to compare against.

Check explicitly, per cell:

- Is the player readable? Scenery must never fully hide them.
- Does art that assumes a background brightness fail on the brightest and darkest cells?
  Silhouettes read at night and become flat stickers in daylight.
- Has contrast dropped anywhere versus the Phase 1 baseline?
- Does the effect actually appear everywhere it should?

Measure cost. Report per-frame timing against the frame budget, and state what the
measurement includes and excludes.

## Phase 4 — Lock in

For every value tuned by eye, add a test asserting its **range or ceiling** — not its
exact value. Effect intensity creeps upward across a project one reasonable-seeming tweak
at a time, and nobody notices the commit that crossed the line. The assertion is what
stops it.

## Diagnostic rule

When output looks wrong, determine whether it is a **bug or a tuning problem** before
adjusting any number. Check first for:

- A layer painting over another — full-surface fills, background clears, gradient washes
  drawn after the thing they should sit behind
- Assignment where accumulation was intended (`alpha = x` instead of `alpha *= x`)
- A transform applied twice, or not at all
- Large-area additive or screen blending compounding into haze

Tuning around a bug bakes the bug in permanently. Fix the draw, then tune.

## Hard constraints

- Never replace authored art with procedural geometry unless explicitly instructed.
  Lines, rectangles, and abstract shapes are not a substitute for artwork.
- When layering an effect over authored art, the effect's job is to **unify, not to
  author**. Good art already carries its own lighting and mood. If the effect is doing the
  heavy lifting, you are fighting the art and the art will lose.
- Never let an overlay reduce the readability of the player character. Fade, silhouette,
  or outline the occluder — readability beats fidelity in an action game.
- Never claim a visual result without having looked at a captured frame of it.
- If you cannot verify visually, say so plainly and state what you verified instead.

## Reporting

State: the capture method used, constraints discovered, the draw order, the matrix
coverage rendered, measured cost, and every value locked behind a test.
