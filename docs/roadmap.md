# Roadmap

The order: first raise the frame rate until a frame is short enough to profile
the GPU properly (Nsight Graphics), then make what is still missing render, then
make that fast. Each item says what was measured and where to start.

## Now: frame rate

The world frame is about 190-250 ms (RTX 3080 Ti, Ryzen 7 7800X3D; see the
README's table). With the CPU and GPU overlapping (`VICTORY_LATE_WAIT=3`) the
GPU does about 78 ms of it, so the processor work around each of the frame's
~33 submissions sets the frame time.

1. **Done: the relay or late wait.** Late wait stage 3 with card buffers from
   1 MiB is the setting (`run.bat`); the relay, alone or with the card buffers,
   was not faster and lost walls before. An Nsight trace of a heavy frame: the
   GPU works about 159 of 314 ms, shader cores idle 78%, no costly shader.
2. **Done (25 November): 256 lists in the air.** At 128 the frame's lists
   reached the limit once a frame and everything in the air was waited for
   (36-42 ms, the GPU then idle). At 256 that settle is gone and the median
   world frame went from 262 to 221 ms with the same image. Left: the settles
   for upload fills (1.5-2 a frame, 20-36 ms), which also wait for
   everything; waiting only for the flights that write what a fill reads
   needs per-flight snapshots (as the relay keeps).
3. **Per-submission processor work**, largest first: refreshes of host copies
   (~43 ms a frame on the presenting thread, ~35 on each worker), deferred
   steps (~33), upload fills (~10), copy-back (~13). (Snapshots and the
   compare went to the GPU with the card buffers.)
4. **Spikes.** Frames of 700-1,400 ms while things stream in; and the title
   draws a few garbage indirect records (all ones, 16,777,216 vertices) that
   the budget cuts off only after several.

5. **Zero-copy: tried, not the way (25 November).** The GPU reading title
   memory itself instead of copies. What was measured:
   - The page pool can be opened by the GPU only as committed sections below
     4 GiB (`VICTORY_POOL_COMMIT=1` splits it; 4,000 MiB opened, 4,096 refused).
     Opening a section keeps all of it in memory, and the sections opened
     stayed committed after the process ended (about 25 GB until a restart).
   - The title maps the pool as some 65,000 single 64 KiB pages: one placed
     buffer covers only the buffers on one run of a section, about 3 ms a frame
     of refreshes. The rest is private memory and windows with holes.
   - Private memory in place (`VICTORY_INPLACE=2`, writes declared to the
     flights): 3,534 buffers and 80 windows moved, their ~70 ms of refreshes
     went, but the GPU read them over PCIe at every use: GPU work 83 -> 192 ms
     a frame, median frame 259 -> 398 ms, streaming slower.
   - Copies overlap little (1.07-1.10x of the title memory they cover).
   So the copies stay in the card's memory; what is left is their cost on the
   processor: per profile run (`copy refresh: one copy` by kind), about 3,500
   refreshes a frame of small private buffers (~70 ms across threads, ~20 us
   each for 60 MB) and ~850 of windows with holes (~64 ms).
   - 26 November: the title's own memory flags (a census in profile builds,
     `VICTORY_PROFILE_MEMORY=1`): write-combined memory (one 2.2 GB
     reservation, `0x2404`) took 80 ms a frame of refreshes, 3,450 small
     buffers for 50 MB; the page pool's views (`0x801`) 68 ms. The small
     write-combined buffers in place, as one heap each, one heap for the
     whole reservation, or 64 MB pieces: typical submissions faster, but
     some took 100-345 ms and the title itself was 70-85 ms a frame slower
     (median frame 221-247 -> 326-368 ms). Not kept.
6. **Next: the title's GPU memory on the card (Resizable BAR).** The console
   has one memory; the nearest a PC comes is the card's own memory written by
   the processor (D3D12 GPU upload heaps). Measured on this machine (26
   November): supported; the processor writes it at 20.7 GB/s and reads it at
   42 MB/s; a mapping lands at ~2.8 TB, above the 1 TiB the title's GPU memory
   must stay below, but with the free address space above 1 TiB reserved for
   the moment it lands at `0xff80000000`. Stages, each measured:
   1. Back the title's write-combined reservations with GPU upload heaps
      (behind a switch): the title writes the card directly; buffers in it are
      placed there (no copy, no refresh, no copy-back; writes still declared
      to the flights). Count every processor read of that memory by path.
   2. Move what still reads it on the processor to the GPU (descriptor sync,
      upload fills, texture detiling, checks), as the reads at 42 MB/s demand.
   3. The page pool (textures, streaming): views of sections, so not
      directly; GPU copies from its pages, or the pool itself on the card.
   Vulkan has the placed mapping this needs built in
   (`VK_EXT_map_memory_placed`); D3D12 only through the reservation trick.
   State (26 November, `VICTORY_CARD_MEMORY=1`): stages 1 and 2 built (the
   reservation on the card, buffers and windows placed there, fills from it
   copied on the card): median 221 to about 165 ms. **Off by default: faces
   render black with it.** Not the fills (`VICTORY_CARD_FILLS=0`), not the
   textures there (93, 54 MB; reads now direct, as the kernel's read refuses
   that memory), not the shader translator; with only the buffers placed
   (`VICTORY_CARD_PLACE=buffers`) faces stay black. Suspected: the GPU reads
   the title's live buffer memory after the title has begun to rewrite it
   (copies took a snapshot at submission). Next: a frame capture of a face's
   draw (`nsight-capture.bat`).
7. **Next: avatarmanz/VIctory-7900XTX's processor-side gains** (measured on
   an RX 7900 XTX, none tied to AMD): descriptor records written per draw
   (280 to 124 ms there), list calls translated on eight threads
   (`VICTORY_THIN`), preparing spread over threads. Their small buffers'
   copies on the card (`VICTORY_CARD_BUFFERS_MIN_KB=64`) were ported and
   measured slower here (166 to 359 ms): off.

Then: a GPU trace in Nsight Graphics (`nsight.bat`, its own cache folder; see
`docs/profiling.md`) once the processor side is near the GPU's time.

## Later: what does not render yet

### Console 64-lane programs on 32-lane GPUs (NVIDIA)

NVIDIA GPUs run waves of 32 lanes. A console program written for waves of 64
is translated to two host waves of 32 when its cross-lane work stays inside
each half; otherwise it keeps `WaveSize(64)` and NVIDIA's driver refuses the
pipeline (`host pipeline refused error=The parameter is incorrect`). AMD GPUs
run 64-lane waves and do not have this problem.

Refused in the world (`VICTORY_WAVE_WHY=1` gives the reasons):

| Program | Group | Why it keeps 64 lanes |
|---|---:|---|
| `sr_tracing:CS_Tiling_Generate_Opaque` (ray-traced lighting tiles) | 64 | LDS read and written in one lane branch, mask as data, `v_mbcnt` |
| `GpuParticleUpdate:CS_Visibility` | 64 | LDS read and written in one lane branch |
| `alphaResolve:CS_AlphTransmissionPrefilterMip` | 256 | the same, `v_readlane` |
| `vfx_triangleClip:CS_CollectTriangles_Meshlets` | 1024 | the same, mask as data, `v_mbcnt` (and 32 KiB of LDS: a split adds to it) |
| `vfx_triangleClip:CS_DefragHeap_ComputeRelocations` | 128 | the same, mask as data, `v_mbcnt`, `v_readlane` |

The common cause: on the console a wave's 64 lanes run in step, so the
program passes values through LDS between them with no barrier. Split into two
host waves they need one, and a barrier cannot go where only some lanes go (the
others would never reach it and the group would hang).

`docs/fork-ray-tracing.md` says the ray-tracing **structure builder**
(`XboxBvhBuilder.dll`) is held back the same way on NVIDIA: it runs all or
none (`VICTORY_BVH_BUILDER=1`), and some of its stages are refused. With it off,
its dispatches are about 600 of the "dropped without a host pipeline" a frame,
and the traced lighting has no structures to trace against (likely much of why
shadowed areas are black).

The way to fix it: run each console wave in **one** host wave, each host lane
carrying two console lanes (lanes n and n + 32). The wave then stays in step,
LDS needs no barriers, and the cross-lane instructions (`v_mbcnt`,
`v_readlane`, masks as numbers) work on both halves at once. Work in
`crates/victory-shader/src/emit/lower.rs` (see the wave64 decision near
`host_max_wave`), with tests like `wave64_tests.rs`. It adds GPU work (the
builder, real traversal in the trace programs), so it comes after the frame
rate.

`shader-info --verbose` prints each compute program's group, wave width and
LDS.

### Also not rendering

- `cascadeshadows_rendering:CS_ShadowRenderTiledCascadesAlphaCBPPPC` (alpha-tested
  cascaded shadows) does not translate: "a lane mask exchanged where not every
  lane runs".
- Some models never appear, in every run so far. Not dropped draws (the drop
  log shows none for models): look at textures and streaming.
- Five shadow-mask trace programs do not translate (`docs/fork-ray-tracing.md`).
