# ForwardVI roadmap

What is open, most pressing first. Each item says what is known; "done" items
leave this file for the commit history.

## Now

- **The crash at the river** (first boat mission). Deterministic: the title's
  script interpreter copies a 16-byte field out of a null object (opcode word
  `0x3128`, handler at game `+0x3de6466`), the same 18-frame stack every time.
  Crash reports now dump the memory around each register; the bytecode is
  obfuscated (opcode word x `0x3315` into a 65,536-entry table at
  `0x146553900`; operand bytes XOR `85 94 a7 b6 c1 d0 e3 f2`, times `0x6d`).
  Next: decode the instructions before the crash from a dump, find the native
  that returned null, and what it reads from us.
- **The black sun** (and its reflection in the water): black on one PC and
  not on another with the same GPU, driver, ReBAR and code. A stale shader
  cache is ruled out. Next: card memory off (`VICTORY_CARD_MEMORY=0`) on the
  affected PC; a Graphics Capture of the sky and water passes.

## Frame rate

- **The river: the submitting thread is the limit.** Nsight Systems: the GPU
  runs 72-87 ms of work in a ~260 ms frame; the submitting thread copies 83 MB
  of title memory into our copies and sends 83 MB up to the card's copies a
  frame (741 pieces), makes 13,374 raw view twins, mirrors descriptor records.
- **A recipe others report from 2 to 14 fps** (12 GB cards), to try one at a
  time at the river: `VICTORY_CARD_BUFFERS_MIN_KB=64` (we have 1 MB),
  `VICTORY_LEVELS_ON_DEMAND=1`, `VICTORY_RAW_VIEWS=0` (check the picture).
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
- **Upscaler mode**: `run.bat opti` (Series S 720p, OptiScaler) is untried.
