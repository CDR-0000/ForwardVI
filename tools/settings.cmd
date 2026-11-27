@echo off
rem The game's switches, shared by run.bat and the nsight-*.bat files: change
rem them here and every way of running the game has them.

rem -- avatarmanz's recommended settings (their scripts\play.ps1) --
set VICTORY_TEXTURE_NO_WAIT=1
rem The relay: the steps that wait for flights done on the card (about 12-14
rem fps at the river against 7 without, 27 November). It runs as
rem victory-scarlett runs it: no card memory, no thin lists, copies not
rem joined, the card not kept awake; with those on, faces and surfaces were
rem black until they loaded in.
set VICTORY_RELAY=2
set VICTORY_RELAY_COPY=1
set VICTORY_WIDE_BACK=1
set VICTORY_CARD_REHASH_RUNS=1
set VICTORY_JOIN_COPIES=0
set VICTORY_INDIRECT_WINDOW=1
set VICTORY_CARD_STEPS=2
set VICTORY_INDIRECT_BATCH=2
set VICTORY_MAP_THREADS=1
set VICTORY_TITLE_PRIORITIES=0
set VICTORY_CARD_AWAKE=0
set VICTORY_SHOW_AFTER=1
set VICTORY_AMBIENT=0.8
set VICTORY_SKIP_PIPELINES=none
set VICTORY_THIN=0
set VICTORY_JOIN_PARTS=8
set VICTORY_UPLOAD_AHEAD=1

rem -- ForwardVI --
rem The pipelines the game makes, written down for precompile.bat's warm step.
set VICTORY_PIPELINE_BOOK=1
rem The frame graph: one bar per frame, in a small window of its own.
set VICTORY_FRAME_GRAPH=1
rem Card memory through Resizable BAR: off, as the relay needs (1 turns it
rem on; without the relay it ran), and avatarmanz's windows in card memory.
if not defined VICTORY_CARD_MEMORY set VICTORY_CARD_MEMORY=0
if not defined VICTORY_WINDOWS_BAR_MB set VICTORY_WINDOWS_BAR_MB=1024
rem Title buffers of 1 MB and more get a copy in the card's memory (up to 1 GB
rem of them), where the card finds what shaders wrote. On cards of 12 GB or less
rem the defaults are 16 MB and 512 MB: at the river the 512 MB were full and the
rem processor snapshotted and compared 294 MB a frame of the copies of 1-16 MB
rem buffers, which the submitting thread waits for (26 November: 3.4 fps,
rem 190 ms a frame waiting for the card while it ran 80 ms of work).
if not defined VICTORY_CARD_BUFFERS_MIN_MB set VICTORY_CARD_BUFFERS_MIN_MB=1
if not defined VICTORY_CARD_BUFFERS_MB set VICTORY_CARD_BUFFERS_MB=1024
rem DirectStorage, the game's streaming: the oldest batch of reads first (the
rem game waits on its oldest when 16 are outstanding) and workers on the
rem physical cores; the staging floor is 256 MB in the code (the game asks 32).
if not defined VICTORY_DSTORAGE_OLDEST_FIRST set VICTORY_DSTORAGE_OLDEST_FIRST=1
if not defined VICTORY_DSTORAGE_WORKERS set VICTORY_DSTORAGE_WORKERS=physical
rem The largest level of big textures (1024 texels and more) left out: a
rem quarter of the bytes to upload, so textures the shaders pick are there
rem sooner (black faces and surfaces that load in late); softer up close.
rem 0 keeps every level.
if not defined VICTORY_TEXTURE_MIP_DROP set VICTORY_TEXTURE_MIP_DROP=1
rem The title's 30 fps cap lifted to 60: our clock and its own setting.
if not defined VICTORY_FRAME_INTERVAL_MS set VICTORY_FRAME_INTERVAL_MS=16.667
rem Being tried (test switches, not set here yet):
rem   VICTORY_NATIVE_SHADERS=1  hand-written shaders (native\README.md) in place
rem                             of their translations
rem   VICTORY_FAST_FLOAT=15     shorter forms of four console float instructions
rem                             (a zero's sign may differ); precompile.bat with
rem                             it set too, as it changes every translation
set VICTORY_LOG=info
