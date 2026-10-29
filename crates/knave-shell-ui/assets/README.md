# Knave logo assets

`knave-color-light.svg` and `knave-color-dark.svg` are the official Knave mark
from [`knave-de/.github`](https://github.com/knave-de/.github), with their paths
and viewBox preserved. The monochrome and high-contrast light SVGs use those
same paths and change only the fills.

The vectors are rasterized ahead of time to checked-in 128×128 transparent PNGs
because the shell image API consumes RGBA pixels. The shell loads those PNGs; it
does not require an SVG renderer at runtime. After editing an SVG, regenerate the
renders with `./render-brand-assets.sh` (requires `rsvg-convert`).
