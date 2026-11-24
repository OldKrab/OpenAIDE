# Vendored fonts

## JetBrains Mono subset

`jetbrains-mono-subset-wght.woff2` is the monospace fallback in `--oa-mono-font`. It serves platforms whose system monospace font lacks box-drawing glyphs, such as Android's Droid Sans Mono. Without it, a fallback font with a different advance width draws those glyphs and diagram columns drift.

The file is a subset of the JetBrains Mono variable font (`fonts/variable/JetBrainsMono[wght].ttf`, release v2.304, upstream commit `19371302b95d`). It is licensed under the SIL Open Font License 1.1; see `JetBrainsMono-OFL.txt`. Fontsource and Google Fonts subsets omit the box-drawing block, so this repository vendors its own subset.

The subset drops layout features so code ligatures never merge characters in code or diagrams. To regenerate it, use fontTools with brotli:

```sh
pyftsubset 'JetBrainsMono[wght].ttf' \
  --unicodes='U+0020-007E,U+00A0-024F,U+0370-03FF,U+0400-04FF,U+2000-206F,U+20A0-20C0,U+2100-214F,U+2190-21FF,U+2200-22FF,U+2300-23FF,U+2500-257F,U+2580-259F,U+25A0-25FF,U+2600-27BF,U+FFFD' \
  --layout-features='' --no-hinting --desubroutinize --flavor=woff2 \
  --output-file=jetbrains-mono-subset-wght.woff2
```

Keep the `unicode-range` of the `@font-face` rule in `tokens.css` equal to the `--unicodes` list.
