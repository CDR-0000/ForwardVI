@echo off
rem The game's switches, shared by run.bat and the nsight-*.bat files: change
rem them here and every way of running the game has them.

rem -- avatarmanz's recommended settings (their scripts\play.ps1) --
set VICTORY_TEXTURE_NO_WAIT=1
rem Their relay flickers black tiles on NVIDIA (26 November): off.
set VICTORY_RELAY=0
set VICTORY_RELAY_COPY=0
set VICTORY_WIDE_BACK=1
set VICTORY_CARD_REHASH_RUNS=1
set VICTORY_JOIN_COPIES=1
set VICTORY_INDIRECT_WINDOW=1
set VICTORY_CARD_STEPS=2
set VICTORY_INDIRECT_BATCH=2
set VICTORY_MAP_THREADS=1
set VICTORY_TITLE_PRIORITIES=0
set VICTORY_CARD_AWAKE=1
set VICTORY_SHOW_AFTER=1
set VICTORY_AMBIENT=0.8
set VICTORY_SKIP_PIPELINES=none
set VICTORY_THIN=1
set VICTORY_JOIN_PARTS=8
set VICTORY_UPLOAD_AHEAD=1

rem -- ForwardVI --
rem The pipelines the game makes, written down for precompile.bat's warm step.
set VICTORY_PIPELINE_BOOK=1
rem The frame graph: one bar per frame, in a small window of its own.
set VICTORY_FRAME_GRAPH=1
rem Card memory through Resizable BAR (ReBAR on in the BIOS; without it the
rem game runs as before), and avatarmanz's windows in card memory on top.
if not defined VICTORY_CARD_MEMORY set VICTORY_CARD_MEMORY=1
if not defined VICTORY_WINDOWS_BAR_MB set VICTORY_WINDOWS_BAR_MB=1024
set VICTORY_LOG=info
