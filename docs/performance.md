# Performance

## Targets
- 60 FPS at 100+ widgets on mid-range mobile and desktop hardware.
- Single-digit draw calls via batching.

## Strategies
- Immediate-mode UI with batched quads.
- Text atlas reused across frames.
- Copy-on-write form state and undo/redo history.
- Spatial hit-test grid for pointer routing.

## Benchmarks
Run CPU benchmarks:

```bash
cargo bench -p ui-core
```

GPU and memory profiling should be done in the host browser via performance tools.

