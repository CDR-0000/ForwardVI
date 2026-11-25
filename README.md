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

On top of ztrx so far:

- **Offline shader and pipeline precompilation**, from
  [fxceless/victory-fork#282](https://github.com/fxceless/victory-fork/pull/282)
  by t4l03: `victory translate --cache --pipelines` fills the shader cache and the
  GPU pipeline library before you play, so the game no longer compiles them while
  it runs. On an RTX 3080 Ti the first full precompile took about 8 minutes and
  built about 23,000 pipelines; loading into the world was much faster afterwards.
- **victory-fork's newer work** up to `310c6eb`: the depth bounds test (the first
  room lit by its lamp), fixes to how textures picked by index are uploaded (the
  room's walls and the characters' textures), and the AMD view-heap clamp.
- **Defaults that draw the correct image.** victory-fork turned on its fast
  modes by default (the relay, wide copy-back, card buffer copies from 1 MiB);
  they lost walls and models in the first room
  ([fxceless/victory-fork#168](https://github.com/fxceless/victory-fork/issues/168)).
  ForwardVI leaves them off until they draw the same image; each still turns on
  with its switch for measuring.

## Status (2026-11-25)

Inherited from ztrx: boots through the intro and is playable through the opening
mission, at 1-3 fps in the world. Anything in shadow is black and
bright scenes overexpose (the game's ray-traced lighting does not run yet).

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

## Faster settings (not yet correct)

`VICTORY_RELAY=2`, or `VICTORY_LATE_WAIT=3 VICTORY_WIDE_BACK=1`, cut frame time
by about 40%, but they currently lose walls and models. Making them draw the
correct image is ForwardVI's first target.

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
