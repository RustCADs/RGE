# Lucide icon subset (vendored)

Subset of the [Lucide](https://lucide.dev) icon library, vendored
2026-05-05 for RGE Wave W06.

Lucide ships 1500+ icons; this initial vendor covers a representative
**~100-icon subset** sufficient for editor toolbars, menus, panels, and
the most common application chrome (file/edit/view/playback/help).
Additional icons can be added by dropping the SVG into this directory
and adding a `name: "lucide/name.svg"` entry to `../lucide.icons.ron`.

## Categories covered

| Category   | Examples                                                         |
| ---------- | ---------------------------------------------------------------- |
| File       | folder, folder-open, save, file, file-plus, file-text            |
| Edit       | edit, pen, pencil, eraser, copy, cut, trash, trash-2, plus, minus|
| Playback   | play, pause, stop, play-circle, pause-circle, stop-circle        |
| Navigation | arrow-{up,down,left,right}, chevron-{up,down,left,right}, home   |
| View       | eye, eye-off, search, zoom-in, zoom-out, sidebar, columns, rows  |
| Layout     | maximize, minimize, panel-{left,right,top,bottom}, grid, list    |
| Transform  | move, rotate, rotate-cw, rotate-ccw, scale, flip-h, flip-v       |
| Status     | check, x, info, alert-circle, alert-triangle, help-circle        |
| Tools      | paint, paintbrush, palette, sliders, filter, eye-dropper         |
| Other      | sun, moon, star, heart, bell, mail, calendar, clock, settings    |

## License

Lucide is distributed under the ISC license (a fork of Feather, MIT).
The full license text is in [`LICENSE`](LICENSE) — both must be
preserved when redistributing.

## SVG format conventions

All vendored icons follow the canonical Lucide format:

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="24" height="24"
     viewBox="0 0 24 24" fill="none" stroke="currentColor"
     stroke-width="2" stroke-linecap="round" stroke-linejoin="round">
  <path d="..."/>
</svg>
```

This format is consumed directly by `crate::tint::apply_tint` (which
substitutes `currentColor` for the requested theme color) and
`crate::tint::rasterize` (which renders to RGBA pixels via
`tiny_skia`). Adding icons in any other format is supported but the
default tinting pipeline assumes `stroke="currentColor"`.
