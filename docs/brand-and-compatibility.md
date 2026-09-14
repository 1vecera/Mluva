# Mluva identity and artwork

**Mluva** means speech or manner of speaking in Czech, pronounced roughly “MLOO-vah.” The product descriptor is **Open-source dictation and rewriting for Omarchy.** Use the same name in application titles, launchers, packages and documentation.

![Mluva on dark and light backgrounds](assets/mluva-brand-sheet.png)

The original abstract mark suggests a thought unfolding into speech. Solid ink and frost variants keep its silhouette legible on light and dark surfaces. The custom wordmark uses JetBrains Mono Medium outlines, optical spacing and a narrowed “l”. Its [font provenance](assets/brand-source/wordmark-provenance.json) and [OFL notice](../linux/quickshell/mluva.dictation/fonts/OFL.txt) are retained. Editable app and film text uses the same JetBrains Mono family.

| Surface | Asset |
| --- | --- |
| Application launcher | [App tile](../linux/resources/com.mluva.Linux.svg) |
| GTK and GNOME panel | [Symbolic icon](../linux/gnome-extension/recording-status@mluva.local/mluva-symbolic.svg) |
| Standalone mark | [Dark](assets/mluva-mark.svg) · [Light](assets/mluva-mark-on-light.svg) |
| Wordmark | [Dark](assets/mluva-wordmark.svg) · [Light](assets/mluva-wordmark-on-light.svg) |
| Horizontal lockup | [Dark](assets/mluva-lockup.svg) · [Light](assets/mluva-lockup-on-light.svg) |
| Repository banner | [Hero](assets/mluva-hero.svg) |

Preserve aspect ratio and clear space. Use the symbol alone at small sizes. Use the solid ink variant on white; light lettering needs a dark background.

Regenerate assets from the committed vector geometry and wordmark outlines:

```sh
make linux-setup
cd linux
uv run --locked python -m mluva_linux.brand_assets --png
```

PNG generation needs `rsvg-convert` from librsvg. `make linux-test` checks generated SVGs for drift. Application colors follow the desktop theme independently of these static assets.

Upgrades preserve existing settings and conversations through the [identity migration](identity-migration.md). [Provider configuration](provider-selection.md) determines where audio and text are processed; the product name does not imply local inference.
