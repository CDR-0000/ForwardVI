# Hand-written shaders

A console program whose translation costs too much on the card can be written
again by hand, in HLSL, and run in place of its translation.

- **File**: `native/<hash>.<name>.hlsl`. `<hash>` is the 16 hex digits a run's
  log names the program by (`hash=` in "first bind", "shader translated");
  `<name>` is free (the program's name from `victory translate --list`, say).
- **Build**: `build.bat` compiles every one with the Windows SDK's `dxc`
  (`tools\native-shaders.cmd`, `-T cs_6_6 -E main`) into
  `target\native\<hash>.dxil`. A shader that does not compile fails the build.
- **Run**: `VICTORY_NATIVE_SHADERS=1` (test switch first, `tools\settings.cmd`
  once proven). The log says how many it found, and names each program as its
  hand-written shader takes its place. Unset or `0`: the translations, as
  before; the shader cache is the same either way.

## What a hand-written shader must keep

Only the DXIL is taken from the file. Everything else comes from the
translation: the root signature, the user data, the samplers, the views, the
thread group size. So the shader must declare **exactly the bindings the
translation declares**, and do what the console program does with them.

Read the translation's interface first:

```powershell
$env:VICTORY_HOST_WAVE='32'; $env:VICTORY_TRANSLATE_ONLY='<hash>'; $env:VICTORY_DUMP_MATCH='logs\native;#<hash>'
target\release\victory.exe translate --list ..\shaders.rpf
$env:VICTORY_EMIT_DUMP='<a folder outside the repository>'
target\release\victory.exe translate --list logs\native
dxc.exe -dumpbin <that folder>\<the .dxil> -Fc <that folder>\shader.ll
```

`shader.ll` lists the bindings (`Resource Bindings`), the thread group and wave
size (`PSVRuntimeInfo`), and the translated code: how each texture's and
buffer's descriptor heap index is reached from the user data. For example, a
compute program of the shadow pass declared:

- `cbuffer` at `b0`, space 0: the user data, one `uint` per console user-data
  dword (8 of them there);
- static samplers `s0`..`s3`;
- everything else from `ResourceDescriptorHeap[...]` (shader model 6.6), at the
  index the translated code computes;
- `[WaveSize(32)] [numthreads(4, 8, 1)]`, and its group-shared memory.

## Rules

- Same results as the translation, within float rounding: check against it
  (a frame of each, same place, compared) before it goes into `settings.cmd`.
- Nothing copied from the game: the HLSL is ours, written from what the
  translated code does (rule #1, `docs/asset-policy.md`).
- Measure it: the frame log (`VICTORY_FRAME_LOG`) and Nsight's GPU Trace, the
  program's time with and without.
