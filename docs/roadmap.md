# ForwardVI roadmap

What is open, most pressing first. Each item says what is known; "done" items
leave this file for the commit history.

## Watch

- **The crash at the river and the black sun went away together** (26
  November), after a new NVIDIA driver, every cache cleared (ours and the
  driver's), a restart and a new build; which of these did it is not known.
  The crash was deterministic: the title's script interpreter copied a 16-byte
  field out of a null object (opcode word `0x3128`, handler at game
  `+0x3de6466`, the same 18-frame stack every time); the sun was black on one
  PC and not on another with the same GPU, driver, ReBAR and code. If either
  comes back: crash reports dump the memory around each register, and the
  bytecode decodes as follows (it is obfuscated): opcode word x `0x3315` into
  a 65,536-entry table at `0x146553900`; operand bytes XOR
  `85 94 a7 b6 c1 d0 e3 f2`, times `0x6d`.

## Frame rate

- **The river** (after the run above): about 195 ms a frame, 5.1-5.3 fps;
  the GPU runs about 125 ms of it (two thirds busy), our preparing 31 ms
  (copies 21 ms, 85 MB), copying back 16 ms. Before (Nsight Systems, ~260 ms
  frames) the submitting thread was the limit: the GPU ran 72-87 ms of work,
  the thread copied 83 MB of title memory and sent 83 MB up to the card's
  copies a frame (741 pieces), made 13,374 raw view twins, mirrored
  descriptor records.
- **The river, Nsight Systems (26 November, take 3; takes 1-2 had the driver
  compiling pipelines on four threads, the cache being cold):** ~250 ms
  frames; the GPU busy 110 ms of each (46 %); no thread saturated (the
  submitting thread runs 33 % of the time, the process 2.3 cores). The frame
  runs in turns: the submitting thread prepares (~65 ms, the card idle
  behind it), then stands waiting for the card 115 ms a frame (our log),
  for flights to land before the next submission. Overlapped, the frame
  would be near max(65, 110) ms.
  - **The relay** (`VICTORY_RELAY=2`, `VICTORY_RELAY_COPY=1`,
    `VICTORY_CARD_STEPS=2`, as victory-scarlett runs) does the steps that
    wait for flights on the card instead: the biggest win in sight, once its
    black tiles flickering on NVIDIA are found and fixed.
  - **Thread wake-ups on the submitting thread**: about half its running time
    is in the kernel waking other threads (`NtAlertThreadByThreadIdEx`):
    `shadow::wide`'s `notify_all` (shadow.rs:1559), `Host::card_sent`
    (host.rs:4337), `descriptor::begin_sync_heaps` (descriptor.rs:269),
    `watch::writes` (watch.rs:886); ~26 ms a frame. `VICTORY_WIDE_SPIN=<us>`
    keeps the wide threads awake between batches (off); test it.
  - ~100 `ExecuteCommandLists` a frame; the GPU submissions themselves
    ~15 ms a frame of the thread.
- **A recipe others report from 2 to 14 fps** (12 GB cards), to try one at a
  time at the river: `VICTORY_CARD_BUFFERS_MIN_KB=64` (we have 1 MB),
  `VICTORY_LEVELS_ON_DEMAND=1`, `VICTORY_RAW_VIEWS=0` (check the picture).
- **Op-lite/victory-scarlett** (branch `scarlett-fox`: cool_fox plus three
  commits; RTX 4080 SUPER 16 GB, 32 GB RAM, 1440p, Series X): 41-61 ms a
  frame on its opening route, about 70 ms in a heavy scene; its own note:
  single runs differ by up to 15 ms. Its three "that made the difference":
  - `VICTORY_FILL_TITLE=1`: in our tree already (off). Small fills the
    title wrote itself stop waiting for flights' copy-backs. Test first.
  - `VICTORY_POOL_DISCARD=1` (their `xmem.rs`): streaming-pool pages the
    title gave back are reset and taken out of the working set while free
    RAM is short. For 32 GB PCs (ours ran at 86-87 % memory load).
  - `VICTORY_PARTIAL_DELTA=1` (their `texture/delta.rs`): a texture found
    changed uploads only the subresources whose bytes changed.
  Its memory set (run-128, less commit, same speed): `BAKE_DELTA`,
  `MAP_LOCK_FINE` (a page-mapping call blocks only readers of its range),
  `STAGING_PACK`/`STAGING_REST_MS`, `SEND_SPARE_MB`, `WINDOW_TAILS`,
  `HEAP_PACK`, `LIST_POOL_IDLE_S`. Its set runs the relay (`VICTORY_RELAY=2`),
  which flickered black tiles here.
- **Card memory fills** are copied by the processor at submission
  (`VICTORY_CARD_FILLS`): the card copying them when the list ran found the
  next frame's bytes. A card copy that takes the bytes of the right frame
  would save the processor's reads.

## Pictures

- **Textures in the console's XOR tiling modes** cannot be copied back into
  title memory (`victory_formats::texture::Layout::addressable`): one water
  texture at the river is skipped. Neptune_VI has the addressing
  (`Layout::for_gpu`, after MiamiGame/XTP) but turns it off: it turned the
  water to foam with black patches, likely the base swizzle.

## Tools

- **Pipeline warm-up (`victory warm`)**: two thirds of the logical cores (10
  processes here). Every core (16) froze the whole PC, the cursor too, though
  the processes run at idle priority: the GPU driver or memory, not the CPU.
  The long tail is a few pipelines of 10-38 s each (p99 17 s): make the known slow ones
  (`slow-pipelines.txt`) first, so they do not finish last; try batches
  smaller than 40 per process (each new process sets up a device and loads
  translations, so measure).
- **Upscaler mode** (`run.bat opti`, Series S 720p, OptiScaler): the first run
  (OptiScaler 0.9.5) sat at the start screens for the 35 s it was given; the
  redirect could not load the PC FidelityFX DLL: that release has no
  `amd_fidelityfx_upscaler_dx12.dll`, and the fallback name
  `amd_fidelityfx_dx12.dll` was not there either. Find which file the release
  ships (or take one from a PC game) and point VICTORY_FSR_DLL at it; DLSS
  also needs `nvngx_dlss.dll` beside OptiScaler.
