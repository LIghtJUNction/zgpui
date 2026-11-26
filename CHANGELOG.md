# Changelog

## 0.1.9

- Native windows can set a minimum client size independently of the launch size.
- Native hosts can fit and stack two panes, stretch a preview, scroll overflow,
  and wrap equal-width controls according to the available container width.
  See [responsive layout](docs/native.md#resizing-the-window-and-its-contents).

## 0.1.7

### Added

- Explicit native material descriptions, quality and texture-cache budgets,
  content revision keys, actual capability queries and resource counters.
- In-window frosted, progressive-blur, mica-style, liquid-glass and paper
  surfaces, with a bounded custom GLSL fragment-function registration API.
- Separate screen-space backdrop sampling and captured-content sampling for
  perspective folds, including text captured as part of a complete layer.
- An interactive `materials` gallery with strength, quality and openness
  controls, reported fallbacks, cache statistics and a cancellable 1.2-second
  animation. Idle operation waits for events without continuously redrawing.
- A bounded JSON benchmark and a repetition runner that preserve CPU submission
  time, synchronized frame wall time, texture storage and capture/pass counters.

The materials are renderer effects inside the application's own window. They
do not claim native DWM Mica, Apple compositor integration or a cross-platform
performance advantage. See [materials](docs/materials.md) for the API contract,
reproduction commands and validation boundaries.
