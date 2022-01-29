# GB Packing Visualizer

A Rust program for visualizing how sections get packed into your Game Boy ROM.

<video controls muted preload="auto" src="https://raw.githubusercontent.com/wiki/ISSOtm/gb-packing-visualizer/rhythm_land.mp4"></video>

## Caveats

This program is usable, but definitely not production-ready. (...yet?)
- Generating the logs this program needs requires manually patching RGBDS' source code, and re-compiling.
- Encoding the video is kind of slow.
  This is despite my efforts to the contrary; it simply seems that the encoding process is slow, and I can't do much in that regard given the tools at my disposal (and the time I'm willing to invest into this, as well).
- The generated files are... suboptimal?
  File size can be *halved* by simply passing the video through `ffmpeg`.
  It's not great, but should be acceptable imo, especially as FFMpeg works through the task at 9× the playback speed.

## Usage

The outline is to first generate the linking log, then render the video from that.

### Log generation

Generating the log requires accessing some of RGBLINK's internal state, so you must patch it.
1. Obtain a copy of [the source code](https://github.com/gbdev/rgbds)
2. Apply the patch `link-logs.patch` to the source code (if you get told it cannot be applied, it was created from commit 20a26599a3de9fa7c24f8daef7310721b2c2958a)
3. Compile RGBDS
4. Link the project you want to visualize, storing RGBLINK's standard output to some file.
   How to do this is dependent on the project, but here are some common cases:
   - **Build script** (`build.sh`, `build.bat`, etc.): modify the script to use your custom RGBDS.
     Then, run the script.
   - **`Makefile`**: delete the ROM, and then link it again with your new RGBDS.
     This can usually be done either by outright modifying the Makefile, or often just by overriding some variable when calling `make` (e.g. `make "RGBLINK=$HOME/rgbds/rgblink >/tmp/link.log"`).
   Be careful that RGBASM and RGBLINK's versions are usually fairly tightly coupled, so if you get an error about a bad object file format, try re-compiling from scratch with the custom RGBASM and RGBLINK.

### Rendering

5. Compile this program (`cargo build --release`).
   **Compiling in release mode is strongly advised**, as it provides a **noticeable** performance boost.
   (Easily 2×, I'd say!)
6. Run this program, redirecting its standard input from the linking log, and passing the output video file name as the sole argument: `cargo run --release vid.mp4 < link.log`
7. Wait a bit.
   The program will report what it's currently doing, the longest part of which is the actual rendering.
8. Optional, but **strongly recommended**: pipe the video through [FFMpeg](https://ffmpeg.org) (`ffmpeg -i vid.mp4 vid_better.mp4`), which should yield a smaller file that looks just the same.
   FFMpeg being very good at its job, this should be significantly faster than the rendering.
9. Profit!
