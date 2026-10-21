# Window clipping patches

`gpui` and `gpui_wgpu` are source snapshots from
`l0ng-ai/zed` commit `fb4472347db56abd807008a7a9c5557474679363`
(the `tty7` branch already used by this workspace). They retain their Apache-2.0
licenses. Examples, benchmarks, and upstream integration tests are omitted;
their manifests expand the original workspace dependencies.

The local changes add a per-window rounded clip to GPUI's `Window` and `Scene`,
then apply its signed-distance alpha mask to WGPU fragment outputs. The frame's
drop shadow is exempt. This clips text, images, panel fills, paths, and overlays
without changing their layout or adding content padding. The clip is scaled to
device pixels on every frame, including cached view redraws.

Modified upstream files:

- `gpui/src/window.rs`: retain and submit the clip with each frame.
- `gpui/src/scene.rs`: carry the clip to the renderer.
- `gpui_wgpu/src/wgpu_renderer.rs`: upload clip bounds and radii in uniforms.
- `gpui_wgpu/src/shaders{,_subpixel}.wgsl`: apply the mask to fragment outputs.
- `gpui_wgpu/src/window_clip_tests.rs`: shader validation and GPU pixel tests.

Run `cargo test -p gpui_wgpu window_clip` for shader validation and GPU regression
tests. The pixel test uses a surface-free Vulkan adapter, including software
Vulkan, and checks both 1x and 2x scale. Existing UI layout tests remain in tty7.

When updating the fork revision, refresh both snapshots together and reapply
these changes, or remove the path patches once the fork includes this API.
