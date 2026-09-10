# Mluva identity and artwork

**Mluva** means speech or manner of speaking in Czech, pronounced roughly “MLOO-vah.” The product descriptor is **Open-source dictation and rewriting for Omarchy.** Use the same name in application titles, launchers, packages and documentation.

![Mluva on dark and light backgrounds](assets/mluva-brand-sheet.png)

The flowing-m mark suggests a voice wave. The frost/slate palette fits dark surfaces, and solid ink variants work on light backgrounds. The wordmark uses outlined Adwaita Sans SemiBold; it needs no installed font. Its [font provenance](assets/brand-source/wordmark-provenance.json) and [SIL Open Font License](assets/brand-source/Adwaita-Sans-LICENSE.txt) are retained.

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
