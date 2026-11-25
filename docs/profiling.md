# Profiling

Two tools answer "what does a frame do, and where does its time go":

- **Frame traces** for the CPU side: a timeline of every frame on every thread,
  opened in Perfetto.
- **PIX** for the GPU side: every draw of one captured frame, what it drew and
  how long it took.

## Frame traces (CPU, every frame)

Build with the `profile` feature (its own target folder, so the normal build is
untouched):

```powershell
cargo build --release -p victory --features profile --target-dir target\profile
```

Run that build with `VICTORY_PROFILE=<file.json>` (default `profile.json` in the
working folder). It writes a Chrome-format trace, flushed every two seconds, so
a run that is killed still leaves a usable file. Open it at
<https://ui.perfetto.dev> (drag the file in; it stays in your browser).

What it holds:

- **Frames**: a span named `frame` on the presenting thread, from one present to
  the next, numbered.
- **Steps** (spans of target `profile`, added with `profile_scope!` in
  `victory-gpu`): `submission`, `send to the card`, `wait for the card to finish`,
  `prepare copies`, `wait for the card`, `copy back`, `deferred steps`, `present`,
  `draw prepare`, `heap buffers`, `heap textures`, `compare and copy back`,
  `texture refresh`, `make pipeline`, `wait for memory value`.

To time another step, put `profile_scope!("name");` at the start of its scope.
Without the feature the macro compiles to nothing, and the spans never reach the
log. A step that runs thousands of times a frame makes the trace very large
(one such step made 2.8 GB in three minutes); mark the loop, not each call.

Why not Tracy: its client is C++ and in this process the title's code ends it
with a fail-fast (0xC0000409) as soon as the title starts, even without its
crash handler, sampling or timer check. The Rust-only trace layer has no such
problem.

### First measurement (RTX 3080 Ti, Ryzen 7 7800X3D, default settings)

A world frame took 792 ms (median). On the presenting thread: 56 submissions a
frame, each waiting for the card to finish before the next (280 ms of waiting);
the rest is CPU work inside the submissions (copy refreshes before each one
206 ms, texture refreshes 92 ms, comparing and copying back what shaders wrote
59 ms). The card waits on the CPU and the CPU waits on the card.

## PIX (GPU, one frame)

Install PIX (`winget install --id Microsoft.PIX -e`). In PIX: **Launch Win32**,
with

- Path to executable: `target\release\victory.exe`
- Working directory: the repository folder (where `victory.toml` is)
- Command line: `run "<game dir>\<the title's exe>" --execute`
- Environment: anything `run.bat` sets (for example `VICTORY_RELAY=0`)

Tick **For GPU capture**, launch, get to the scene, and take a GPU capture. The
capture lists every draw and dispatch with its pipeline, resources and render
targets; **Collect timing data** gives each one's GPU time.
