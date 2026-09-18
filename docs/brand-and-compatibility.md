# Mluva identity and artwork

**Mluva** means speech or manner of speaking in Czech, pronounced roughly “MLOO-vah.” The product descriptor is **Open-source dictation and rewriting for Omarchy.** Use the same name in application titles, launchers, packages and documentation.

![Mluva on black and white backgrounds, with the mark at small sizes](assets/mluva-brand-sheet.png)

The logo set lives in [docs/brand](brand/README.md) and was chosen on 18 September 2026. The mark is a glossy red drop of speech: a soft, slightly leaning blob that also reads as a recording light. The wordmark sets **Mluva** with a traced soft M and geometric lowercase “luva”. The default lockup places the mark at 1.05 times the M height to the left of the wordmark, so the mark reads like a letter of the word; medium (1.5) and large (2.25) mark ratios exist for dialogs, cards and about screens.

## Surfaces

| Surface | Asset | Source in docs/brand |
| --- | --- | --- |
| Application launcher | [App tile](../linux/resources/com.mluva.Linux.svg): the glossy mark on a black rounded tile | composed from `svg/mluva-mark.svg` |
| GTK and GNOME panel | [Symbolic icon](../linux/gnome-extension/recording-status@mluva.local/mluva-symbolic.svg): the silhouette in the theme foreground | copy of `svg/mluva-mark-symbolic.svg` |
| Standalone mark | [Glossy mark](assets/mluva-mark.svg); flat (`#E91B27`) and symbolic variants stay in `brand/svg/` | copy of `svg/mluva-mark.svg` |
| Wordmark | [On dark](assets/mluva-wordmark.svg) · [On light](assets/mluva-wordmark-on-light.svg) | copies of `svg/mluva-wordmark-*.svg` |
| Horizontal lockup | [On dark](assets/mluva-lockup.svg) · [On light](assets/mluva-lockup-on-light.svg) | copies of `svg/mluva-logo-*.svg` |
| Repository banner | [Hero](assets/mluva-hero.svg): default lockup and tagline on black | composed from `svg/mluva-logo-on-dark.svg` |
| Site favicon | [favicon.ico](favicon.ico) beside `index.html`, with the SVG mark and `brand/png/mluva-mark-192.png` as touch icon | copy of `mluva.ico` |
| Icon PNGs | `brand/png/mluva-mark-{16..1024}.png` | cut from the raster master |

## Colours and tone

| Token | Value | Use |
| --- | --- | --- |
| Ink on dark | `#F5F5F5` | Default tone: the `-on-dark` files and the unsuffixed copies in `docs/assets` |
| Ink on light | `#171717` | The `-on-light` files, only on light surfaces |
| Flat red | `#E91B27` | Mean colour of the rendered glossy mark; flat mark, single-colour contexts, accents |
| Background | `#000000` | When a surface needs an opaque background: app tile, hero, site |

Backgrounds are transparent by default. Every viewBox is tight, so keep clear space of at least half the mark height around a lockup and scale it as one object. The glossy mark holds down to about 24 px; below that use the flat or symbolic mark. Application colours follow the desktop theme independently of these static assets.

## Regenerate

`docs/brand` is the source of truth. Rebuild it from its committed sources, then refresh the integration surfaces:

```sh
uv run docs/brand/build.py
make linux-setup
cd linux
uv run --locked python -m mluva_linux.brand_assets --png
```

`build.py` needs `rsvg-convert` (librsvg, the renderer GTK uses) and `magick`; `brand_assets.py` needs `rsvg-convert` for the PNG siblings. `make linux-test` checks the app tile, the symbolic icon, the `docs/assets` copies and `docs/favicon.ico` for drift against `docs/brand`.

The wordmark is traced and outlined artwork; it no longer derives from a font. The bundled JetBrains Mono family remains the application typeface under its [OFL notice](../linux/quickshell/mluva.dictation/fonts/OFL.txt). The former frost-blue unfolding-thought mark and JetBrains Mono wordmark were retired on 18 September 2026 and remain in git history.

Upgrades preserve existing settings and conversations through the [identity migration](identity-migration.md). [Provider configuration](provider-selection.md) determines where audio and text are processed; the product name does not imply local inference.
