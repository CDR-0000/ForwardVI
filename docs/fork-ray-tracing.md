# Ray tracing in this fork: what the title does and what has been built

Status, 2026-11-26: **the structure builder builds correct structures on an
NVIDIA card**, checked without the game (`victory bvh-check`, below); nothing is
lit by ray tracing in the game yet. Everything here is off by default.

## The headless check (`victory bvh-check`)

Loads the title's `XboxBvhBuilder.dll` through the loader and runtime (no
title), makes the device, a queue, a list and title buffers as the title does
(`victory_gpu::testbed`), asks the builder for a bottom-level structure over a
test mesh (`--size N`: N x N quads; `--boxes`: a ground with two boxes), runs
the list, and checks the result on the CPU with `victory_shader::bvh`: every
input triangle reached once, every box holds what lies under it, and 400 rays
find the same nearest hit as a test of every triangle. It writes a picture
(sun light and ray-traced shadows, through the structure and by every
triangle) to the cache folder. `VICTORY_BVH_STOP_AFTER=<k>` runs only the
builder's first k dispatches, to look at what each stage writes.

What the structures look like, as this builder writes them: a header with the
root's node pointer at dword 12 and the bounds at dwords 4-9; 128-byte box
nodes (32-bit floats); 64-byte triangle nodes of one triangle (dword 12 = 1) or
of an input quad's two (dword 12 = 0x81, triangle numbers in dwords 13 and 14,
vertex maps in dword 15), the parent's pointer naming the first.

What it found (26 November), all fixed: `DispatchX` counts in threads; the
`USER_DISPATCH_THREADS_X` and `USER_ROOT_ADDRESS` values the builder's programs
ask for; its build records in list upload memory from the process heap, past
the 40 bits shaders address (its first stage read zeros and wrote no header:
`crate::upload`, from avatarmanz/VIctory-7900XTX); and the part of a buffer
that ends inside a 64 KiB page of the directory of memory by address, which
was unreachable (meshes over 5,461 triangles lost the rest). Passes from 2 to
131,072 triangles (the largest built in 56 ms).

## What the title does

The title's indirect light (probes, bounce, reflections) and the
shadows of its lamps and sun are ray traced. Without them, anything the sun
does not hit directly is black: interiors, the shadowed side of a face, rooms
lit only by lamps.

The title does not use a ray-tracing API:

- Traversal is ordinary compute-shader code: a per-thread stack in shared
  memory, and loads through raw 40-bit addresses of title memory.
- The static world's acceleration structures ship pre-built in the title's
  data, in the console GPU's native node format (64-byte nodes: box nodes with
  four children and 16-bit float bounds, 128-byte box nodes with 32-bit float
  bounds, triangle nodes, instance nodes). They are unpacked on the GPU.
- A set of builder programs (from the title's `XboxBvhBuilder.dll`, 52 compute
  programs) builds and refits the dynamic structures on the GPU every frame.
- Only the per-node test is hardware: the instructions
  `image_bvh64_intersect_ray` and `image_bvh_intersect_ray`. One call tests a
  ray against a box node's four children (returning the hit children, nearest
  first) or against a triangle node (returning the intersection's numerator,
  denominator and barycentric numerators).

So mapping this onto the host's own ray tracing (DXR) is not practical: the
layer would have to know the geometry behind structures the title never
describes to it, and the traversal loops live in 64 of the title's own
programs. This fork runs the title's builder and traversal as translated
shaders and emulates the one instruction.

## What this fork adds

1. **The builder programs translate** (49 of 52; all that the title creates in
   the scenes tested). They pass buffer addresses in a spilled parameter table
   rather than in descriptors. Added: the spilled user-data table, root
   unordered-access views (translator and host binding), byte and 16-bit buffer
   accesses, and several control-flow shapes.
2. **Title memory by raw address from shaders.** A directory with one entry per
   64 KiB of title address space leads a shader to the layer's host copy of
   that memory; loads, stores, atomic add/sub and float min/max go through it.
   Before a dispatch the host makes ready the memory its root views and root
   constants name.
3. **The ray test, emulated in DXIL.** Four slab tests and a sorting network
   for box nodes; a watertight ray/triangle test for triangle nodes; both forms
   of the instruction. A plain Rust reference lives in
   `crates/victory-shader/src/bvh.rs`; tests run the emitted DXIL on the GPU
   over 311 ray/node cases and compare with the reference bit for bit.
4. **The trace and shading programs translate** (59 of 64 trace programs, and
   the irradiance-cache shading program). This needed lane masks treated as
   plain numbers on some paths, and exact forms for programs written for
   64-lane waves running on a GPU with 32-lane waves.
5. **A safety rule on the host side.** The builder's stages run only while
   every builder pipeline the title has created has a host pipeline; otherwise
   none run (`VICTORY_BVH_BUILDER=1` asks for it; `VICTORY_BVH_QUARANTINE=1`
   also keeps the builder's stores out of title memory, for a first test). A
   partly working builder wrote garbage bounds into title memory and crashed
   the title.

## Known gaps

- With `VICTORY_BVH_BUILDER=1` the builder still stays off in a real run: a few
  of its pipelines are refused by the host (they still ask for 64-lane waves on
  a 32-lane GPU) or do not translate.
- Five shadow-mask trace programs do not translate yet.
- An access to a title page that has no host copy is lost (a read gives zero, a
  store is dropped).
- Not publicly documented, and assumed: the instruction's rounding, the order
  of two boxes entered at the same distance, and the result for a ray exactly
  through an edge.
- Cost on the GPU is unmeasured.

## Where to look

- `crates/victory-shader/src/bvh.rs`: the reference for the node test.
- `crates/victory-shader/src/emit/lower.rs`: the lowering of the two
  instructions and of memory by address (`Lower::memory`).
- `crates/victory-shader/src/resource.rs`: address tracking for descriptors
  and raw addresses.
- `crates/victory-gpu/src/pipeline.rs`, `draw.rs`: the builder rule and the
  host side of root views and title memory.
