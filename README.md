# ForwardVI

**ForwardVI** runs the Xbox Series build of the game natively on Windows PCs
with NVIDIA graphics cards, from game files you supply yourself.

**This repository contains no game files or assets.** You need your own copy
of the game. Not affiliated with or endorsed by its developer or publisher.

## Goal

**Raise the frame rate to a playable level while making sure everything renders
properly.** Nobody has yet gone past 3-4 fps in the world, and a faster frame
that draws less, or draws it wrong, is not a gain.

How changes get in:

- **Measured.** Every performance change comes with a before/after measurement on
  the same scene, ideally switched within one launch so that scene and timing
  differences cancel. A change that shows no gain does not go in, however good it
  looks on paper.
- **Checked on screen.** A change that speeds things up by skipping or breaking
  rendering is a regression, not a gain.
- **Taken from the whole VIctory family.** Good changes from VIctory, victory-fork,
  ztrx and VItoriaBr (merged or still open) are brought in when they hold up under
  the two rules above, with credit to their authors.

Pull requests and issues are open to prior contributors. A performance change
needs its before/after measurement and screenshots showing the image unchanged.

## Built upon

ForwardVI is a fork of [**ztrx**](https://github.com/Zettrox/ztrx) (Zettrox),
chosen for its stability. ztrx is in turn built on
[**victory-fork**](https://github.com/fxceless/victory-fork) (fxceless) and
[**VIctory**](https://github.com/Marvisak/VIctory) (Marvisak). Almost all of the
work in this tree is theirs.

On top of ztrx so far, each with credit to its authors:

- **Offline shader and pipeline precompilation**, from
  [fxceless/victory-fork#282](https://github.com/fxceless/victory-fork/pull/282)
  by t4l03: `victory translate --cache --pipelines` fills the shader cache and the
  GPU pipeline library before you play, so the game no longer compiles them while
  it runs. On an RTX 3080 Ti the first full precompile took about 8 minutes and
  built about 23,000 pipelines; loading into the world was much faster afterwards.
- **victory-fork's newer work** up to `9376fd6` (fxceless): the depth bounds test
  (the first room lit by its lamp), textures picked by index uploaded right (the
  room's walls, the characters' skin and hair), the AMD view-heap clamp, a write
  through one view of a page seen at its other views, and less processor work per
  submission.
- **The texture guard fix** (Carlos Sant'Anna, joroyt): memory the kernel writes
  is held open until it has written, and writes through every view of a page are
  seen. This ended the intro movie's stutter. Kept when victory-fork's own version
  of the same code was merged.
- **No blending on integer render targets**, from
  [fxceless/victory-fork#51](https://github.com/fxceless/victory-fork/pull/51)
  (a port of VIctory #806): D3D12 refuses a pipeline that blends into an integer
  target, and the world's wall pipeline does; it is now built, as the console
  draws it.
- **Cheaper indirect draws**, from
  [fxceless/victory-fork#220](https://github.com/fxceless/victory-fork/pull/220)
  by Vyacheslav Venglovsky: no barrier after every indirect draw, the draw budget
  charged once per thread group and kept in the card's memory. With windows
  (`VICTORY_INDIRECT_WINDOW=1`) 3,200 draws went from 38.9 ms to 0.94 ms of GPU
  time on an RTX 3080 Ti. Windows were checked in the world
  (`VICTORY_INDIRECT_WINDOW_CHECK=1`): 0 stale records in 50,021 draws.
- **ForwardVI's own work:**
  - **Frame traces** (the `profile` feature, opened in Perfetto) that time every
    step of every frame on every thread; see `docs/profiling.md`. Everything
    below was found and checked with them.
  - **More of the frame on all cores:** texture refreshes, copy refreshes,
    snapshots and the compare of what shaders wrote run on worker threads
    (`VICTORY_WIDE_TEXTURES`, `VICTORY_WIDE_REFRESH`, `VICTORY_WIDE_BACK`), and
    texture reads no longer queue on the mapping lock
    (`VICTORY_TEXTURE_NO_WAIT`).
  - **Copy-back in few writes:** what shaders wrote goes back to the game's
    memory in one write per stretch, not one system call per changed run (the
    victory-fork merge had made that step 703 ms a frame; it is about 40).
  - **A timing run** that names each slow GPU dispatch with its group count and
    group size.
- **Defaults that draw the correct image.** victory-fork turned on its fast
  modes by default (the relay, wide copy-back, card buffer copies from 1 MiB);
  they lost walls and models in the first room
  ([fxceless/victory-fork#168](https://github.com/fxceless/victory-fork/issues/168)).
  ForwardVI leaves them off until they draw the same image; each still turns on
  with its switch for measuring.

## Status (2026-11-25)

Boots through the intro and is playable through the opening mission. Anything in
shadow is still dark and bright scenes overexpose (the game's ray-traced lighting
runs as plain compute work, without the card's ray-tracing hardware).

**Frame time in the world**, RTX 3080 Ti and Ryzen 7 7800X3D, the median of the
last 30 frames of a run stopped 10-20 seconds into the 3D scene:

| Step | Median frame | Frames a second |
|---|---:|---:|
| Starting point (ztrx with the precompile and victory-fork `310c6eb`, default switches) | 903 ms | 1.1 |
| + work on all cores, texture reads without the mapping lock | 581 ms | 1.7 |
| + victory-fork's newer work and the copy-back in few writes | 540 ms | 1.9 |
| + #51, #220 and indirect windows | 253 ms | 4.0 |
| + the CPU and GPU overlapping (`VICTORY_LATE_WAIT=3`) | **191 ms** | **5.2** |

Then buffers of 1 MiB and up were kept in the card's memory (the card finds what
shaders changed in them, so the CPU takes no snapshot of them and compares
nothing): in the heavier scenes (3,000+ draws a frame) copying back went from 77
to 47 ms a frame and the median frame from 315 to 298 ms. The in-game frame
graph shows about 5 fps (a 200 ms median) in the world.

Where the time goes now: an Nsight Graphics trace of one heavy frame has the GPU
working about 159 ms of it and its shader cores idle 78% of the time, with no
single costly shader (the largest job is 10 ms). The GPU is not the limit: the
processor and the GPU still take turns, the processor waiting for the GPU to
empty about once a frame and the GPU waiting while the processor prepares. The
relay, which keeps the GPU fed, was tried again (with and without the card
buffers) and was not faster, so the next work is the processor's.

Since then (same machine, profile builds with a GPU timeline beside the CPU
threads, about 130 world frames a run):

- victory-fork's submit thread (`VICTORY_ASYNC_SUBMIT=1`) and fills that do not
  wait (`VICTORY_FILL_WAIT=1`) are merged and on in the recommended switches.
- Streaming bursts (2,000-5,000 new textures in one frame) stalled the game for
  0.5-3 s. Upload memory now comes from reusable 64 MB slabs instead of one
  allocation per texture, textures that are only uploaded skip zeroing, and new
  textures are made on several threads at once. Stall frames went from 884 to
  about 590 ms, the 90th percentile from 516 to about 440 ms, and buffers made
  during a burst from 126 a frame to none or nearly none.
- Normal frames sit at about 255 ms with the GPU busy only about 80 ms of it:
  most of the rest is keeping copies of game memory in sync with the GPU.

The next step is a leaner design rather than more patching: the GPU reading
game memory directly (no copies to refresh or copy back, and much less RAM),
with textures converted on the GPU. It is built beside the current path behind
a switch and measured before anything is removed.
What comes next, and what does not render yet, is in `docs/roadmap.md`.

We might show up on PortWatch VI soon at this rate.

## Requirements

- Windows 10/11 x64, a CPU with AVX2
- A Direct3D 12 GPU with shader model 6.6 and mesh shaders (NVIDIA RTX 20 series or newer)
- Rust (the version in `rust-toolchain.toml`)
- Several GB of free Windows commit memory (raise the paging file if `crashes.log` reports error 1455)

## Build

```powershell
cargo build --release -p victory
```

Paths (game folder, cache, saves) go in `victory.toml`; see
`victory.example.toml`. Keep the cache and saves outside the game folder.

## Precompile (once, and after updates)

```powershell
target\release\victory.exe translate --cache --pipelines <game dir>\shaders.rpf <game dir>\<the title's exe> <game dir>\XboxBvhBuilder.dll <game dir>\amd_fidelityfx_upscaler_xbox.dll
```

Later runs only translate and build what is new. The pipeline library is specific
to your GPU and driver.

## Play

```powershell
target\release\victory.exe run <game dir>\<the title's exe> --execute
```

or `scripts\play.ps1 -Game "<game dir>\<the title's exe>"`, which keeps the cache
and saves in folders you choose (`-Cache`, `-Data`). Controller: XInput pads (a
DualShock/DualSense through DS4Windows works). With no pad connected the keyboard
acts as one while the game window has focus.

## Recommended switches

These draw the same image and make the frame much shorter; set them before `run`
(the local `run.bat` does):

| Switch | Effect |
|---|---|
| `VICTORY_RELAY=0` | the relay off (it loses walls and models) |
| `VICTORY_WIDE_TEXTURES=1` | a submission's texture refreshes on the worker threads |
| `VICTORY_WIDE_REFRESH=1` | host copy refreshes on the worker threads |
| `VICTORY_WIDE_BACK=1` | the compare of what shaders wrote on the worker threads |
| `VICTORY_TEXTURE_NO_WAIT=1` | texture reads do not wait for the mapping lock |
| `VICTORY_INDIRECT_WINDOW=1` | indirect draws clamped 16-64 records a pass |
| `VICTORY_LATE_WAIT=3` | the CPU and GPU overlap: submissions go to the GPU without a wait for each |
| `VICTORY_CARD_BUFFERS_MIN_MB=1` `VICTORY_CARD_BUFFERS_MB=1024` | buffers of 1 MiB and up kept in the GPU's memory, where it finds what shaders changed |
| `VICTORY_ASYNC_SUBMIT=1` | submissions prepared and sent on a thread of their own |
| `VICTORY_FILL_WAIT=1` | fills of upload memory checked instead of waited for |

Not recommended yet: the relay (`VICTORY_RELAY=2`), which lost walls and models
before and was not faster in the latest runs, and more than 128 lists in the air
(`VICTORY_LATE_WAIT_LISTS=256`), which made frames slower.

## Frame times

`VICTORY_FRAME_GRAPH=1` opens a small window beside the game with one bar per
frame (green up to 100 ms, yellow up to 250, red above), and above it the frames
a second from the median of the last two seconds, the last frame, the worst 1%
of the last ten seconds and the draws a frame. `VICTORY_FRAME_LOG=<file>` writes
every frame's time and draws to a CSV file. Both cost the renderer next to
nothing (the presenting thread adds two numbers under a lock; the graph has a
thread of its own).

## Debugging switches

| Switch | Effect |
|---|---|
| `VICTORY_CAPTURE=<dir>` + `VICTORY_CAPTURE_FRAMES=<list>` | save those frames as PNG (diagnostics build) |
| `VICTORY_KEYS=30:Enter,35:X` | press keys at those seconds (diagnostics build) |
| `VICTORY_D3D12_DRED=1` | on a GPU failure, report the commands in flight |
| `VICTORY_PIPELINE_WAIT_MS=24` | shorter per-frame wait for pipelines still being built |

Diagnostics build: `cargo build --profile diag -p victory --features diagnostics`

## License

GPL-3.0-or-later (see `LICENSE`), as the projects it builds on.
